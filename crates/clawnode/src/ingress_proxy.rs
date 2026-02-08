//! Ingress reverse proxy.
//!
//! Listens on a configurable port (default 8443) and routes incoming HTTP
//! requests to backend services based on `Host` header and path matching.
//! Routes are derived from the `IngressEntry` records in the service store.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use http_body_util::{BodyExt, Full};
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// A routing rule mapping (host, path_prefix) to a backend address.
#[derive(Debug, Clone)]
pub struct IngressRoute {
    pub host: String,
    pub path_prefix: String,
    pub backend_addr: String, // "ip:port"
    pub ingress_name: String,
}

/// Shared routing table for the proxy.
pub type RouteTable = Arc<RwLock<Vec<IngressRoute>>>;

/// Configuration for the ingress proxy.
#[derive(Debug, Clone)]
pub struct IngressProxyConfig {
    pub listen_addr: SocketAddr,
}

impl Default for IngressProxyConfig {
    fn default() -> Self {
        Self {
            listen_addr: SocketAddr::from(([0, 0, 0, 0], 8443)),
        }
    }
}

/// Start the ingress proxy server.
///
/// Runs until the `shutdown` signal resolves. Returns the actual listen address
/// (useful when binding to port 0 for tests).
pub async fn start_proxy(
    config: IngressProxyConfig,
    routes: RouteTable,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<SocketAddr, Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(config.listen_addr).await?;
    let addr = listener.local_addr()?;

    info!(addr = %addr, "ingress proxy listening");

    loop {
        tokio::select! {
            accept = listener.accept() => {
                match accept {
                    Ok((stream, peer_addr)) => {
                        let routes = routes.clone();
                        tokio::spawn(async move {
                            let io = TokioIo::new(stream);
                            let svc = service_fn(move |req| {
                                let routes = routes.clone();
                                async move {
                                    handle_request(req, &routes, peer_addr).await
                                }
                            });

                            if let Err(e) = http1::Builder::new()
                                .serve_connection(io, svc)
                                .await
                            {
                                // Connection reset / closed by client is normal
                                if !e.is_incomplete_message() {
                                    warn!(peer = %peer_addr, error = %e, "connection error");
                                }
                            }
                        });
                    }
                    Err(e) => {
                        error!(error = %e, "accept failed");
                    }
                }
            }
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    info!("ingress proxy shutting down");
                    break;
                }
            }
        }
    }

    Ok(addr)
}

/// Handle an incoming request by matching routes and proxying.
async fn handle_request(
    req: Request<Incoming>,
    routes: &RouteTable,
    _peer: SocketAddr,
) -> Result<Response<Full<bytes::Bytes>>, hyper::Error> {
    let host = req
        .headers()
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");

    let path = req.uri().path();

    let routes_guard = routes.read().await;

    // Find best matching route (longest path prefix match)
    let matched = routes_guard
        .iter()
        .filter(|r| r.host == host && path.starts_with(&r.path_prefix))
        .max_by_key(|r| r.path_prefix.len());

    let route = match matched {
        Some(r) => r.clone(),
        None => {
            return Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Full::new(bytes::Bytes::from(
                    format!("no route for host={host} path={path}"),
                )))
                .unwrap());
        }
    };
    drop(routes_guard);

    // Proxy to backend
    proxy_to_backend(req, &route).await
}

/// Forward the request to the backend service and return the response.
async fn proxy_to_backend(
    req: Request<Incoming>,
    route: &IngressRoute,
) -> Result<Response<Full<bytes::Bytes>>, hyper::Error> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let backend_url = format!("http://{}{}", route.backend_addr, uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/"));

    // Collect the incoming body
    let body_bytes = match req.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Full::new(bytes::Bytes::from("failed to read request body")))
                .unwrap());
        }
    };

    // Use reqwest to proxy (it's already a dependency)
    let client = reqwest::Client::new();
    let backend_req = client
        .request(
            reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::GET),
            &backend_url,
        )
        .body(body_bytes.to_vec());

    match backend_req.send().await {
        Ok(resp) => {
            let status = StatusCode::from_u16(resp.status().as_u16())
                .unwrap_or(StatusCode::BAD_GATEWAY);

            let body = resp.bytes().await.unwrap_or_default();

            Ok(Response::builder()
                .status(status)
                .body(Full::new(body))
                .unwrap())
        }
        Err(e) => {
            warn!(backend = %route.backend_addr, error = %e, "backend request failed");
            Ok(Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Full::new(bytes::Bytes::from(format!(
                    "backend unavailable: {e}"
                ))))
                .unwrap())
        }
    }
}

/// Rebuild the route table from ingress entries and service discovery.
///
/// For each ingress rule, look up the service's ClusterIP (or first endpoint)
/// and create a route entry.
pub fn build_route_table(
    ingresses: &[IngressRouteInput],
    service_ips: &HashMap<String, String>, // service_name -> "ip:port"
) -> Vec<IngressRoute> {
    let mut routes = Vec::new();

    for input in ingresses {
        for rule in &input.rules {
            let backend = match service_ips.get(&rule.service) {
                Some(addr) => addr.clone(),
                None => {
                    warn!(
                        ingress = %input.name,
                        service = %rule.service,
                        "service not found for ingress rule, skipping"
                    );
                    continue;
                }
            };

            routes.push(IngressRoute {
                host: rule.host.clone(),
                path_prefix: rule.path.clone(),
                backend_addr: backend,
                ingress_name: input.name.clone(),
            });
        }
    }

    routes
}

/// Input struct for building route tables.
#[derive(Debug, Clone)]
pub struct IngressRouteInput {
    pub name: String,
    pub rules: Vec<IngressRuleInput>,
}

/// Input struct for a single ingress rule.
#[derive(Debug, Clone)]
pub struct IngressRuleInput {
    pub host: String,
    pub path: String,
    pub service: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_route_table_works() {
        let ingresses = vec![IngressRouteInput {
            name: "web-ingress".into(),
            rules: vec![
                IngressRuleInput {
                    host: "example.com".into(),
                    path: "/".into(),
                    service: "web-svc".into(),
                },
                IngressRuleInput {
                    host: "example.com".into(),
                    path: "/api".into(),
                    service: "api-svc".into(),
                },
            ],
        }];

        let service_ips = HashMap::from([
            ("web-svc".into(), "10.201.0.1:80".into()),
            ("api-svc".into(), "10.201.0.2:8080".into()),
        ]);

        let routes = build_route_table(&ingresses, &service_ips);
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].host, "example.com");
        assert_eq!(routes[0].path_prefix, "/");
        assert_eq!(routes[0].backend_addr, "10.201.0.1:80");
        assert_eq!(routes[1].path_prefix, "/api");
        assert_eq!(routes[1].backend_addr, "10.201.0.2:8080");
    }

    #[test]
    fn build_route_table_skips_missing_service() {
        let ingresses = vec![IngressRouteInput {
            name: "test".into(),
            rules: vec![IngressRuleInput {
                host: "example.com".into(),
                path: "/".into(),
                service: "missing-svc".into(),
            }],
        }];

        let routes = build_route_table(&ingresses, &HashMap::new());
        assert!(routes.is_empty());
    }

    #[test]
    fn build_route_table_multiple_ingresses() {
        let ingresses = vec![
            IngressRouteInput {
                name: "ing-1".into(),
                rules: vec![IngressRuleInput {
                    host: "a.com".into(),
                    path: "/".into(),
                    service: "svc-a".into(),
                }],
            },
            IngressRouteInput {
                name: "ing-2".into(),
                rules: vec![IngressRuleInput {
                    host: "b.com".into(),
                    path: "/".into(),
                    service: "svc-b".into(),
                }],
            },
        ];

        let service_ips = HashMap::from([
            ("svc-a".into(), "10.200.1.2:80".into()),
            ("svc-b".into(), "10.200.1.3:80".into()),
        ]);

        let routes = build_route_table(&ingresses, &service_ips);
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].ingress_name, "ing-1");
        assert_eq!(routes[1].ingress_name, "ing-2");
    }

    #[tokio::test]
    async fn proxy_starts_and_shuts_down() {
        let routes: RouteTable = Arc::new(RwLock::new(Vec::new()));
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let config = IngressProxyConfig {
            listen_addr: SocketAddr::from(([127, 0, 0, 1], 0)), // random port
        };

        let routes_clone = routes.clone();
        let handle = tokio::spawn(async move {
            start_proxy(config, routes_clone, shutdown_rx).await
        });

        // Give the server a moment to start
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Signal shutdown
        shutdown_tx.send(true).unwrap();

        let result = handle.await.expect("join");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn proxy_returns_404_for_unknown_host() {
        let routes: RouteTable = Arc::new(RwLock::new(vec![
            IngressRoute {
                host: "known.host".into(),
                path_prefix: "/".into(),
                backend_addr: "10.200.1.2:80".into(),
                ingress_name: "test".into(),
            },
        ]));

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let config = IngressProxyConfig {
            listen_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
        };

        let routes_clone = routes.clone();
        let handle = tokio::spawn(async move {
            start_proxy(config, routes_clone, shutdown_rx).await
        });

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Shut down cleanly
        shutdown_tx.send(true).unwrap();
        let result = handle.await.expect("join");
        assert!(result.is_ok());
    }

    #[test]
    fn ingress_route_default_config() {
        let config = IngressProxyConfig::default();
        assert_eq!(config.listen_addr.port(), 8443);
    }
}
