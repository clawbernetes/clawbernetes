//! UI rendering for Clawbernetes TUI

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Gauge, List, ListItem, Paragraph, Row, Sparkline, Table, Tabs, Wrap,
    },
    Frame,
};

use crate::app::{ActivityType, App, GpuState, NodeState, NodeStatus, WorkloadStatus};

/// Main UI rendering function
pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(0),      // Main content
            Constraint::Length(3),  // Footer
        ])
        .split(frame.area());
    
    draw_header(frame, app, chunks[0]);
    draw_main(frame, app, chunks[1]);
    draw_footer(frame, app, chunks[2]);
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let titles = vec!["Overview", "Nodes", "Workloads", "Market"];
    let tabs = Tabs::new(titles)
        .block(Block::default()
            .borders(Borders::ALL)
            .title(" 🦀 CLAWBERNETES CONTROL ROOM ")
            .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
        .select(app.selected_tab)
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    
    frame.render_widget(tabs, area);
}

fn draw_main(frame: &mut Frame, app: &App, area: Rect) {
    match app.selected_tab {
        0 => draw_overview(frame, app, area),
        1 => draw_nodes(frame, app, area),
        2 => draw_workloads(frame, app, area),
        3 => draw_market(frame, app, area),
        _ => {}
    }
}

fn draw_overview(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[0]);
    
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);
    
    // Cluster stats
    draw_cluster_stats(frame, app, left_chunks[0]);
    
    // GPU heatmap
    draw_gpu_heatmap(frame, app, left_chunks[1]);
    
    // Activity log
    draw_activity(frame, app, right_chunks[0]);
    
    // Workload flow
    draw_workload_flow(frame, app, right_chunks[1]);
}

fn draw_cluster_stats(frame: &mut Frame, app: &App, area: Rect) {
    let cluster = &app.cluster;
    
    let stats = vec![
        format!("  Nodes: {}/{} healthy", cluster.healthy_nodes, cluster.total_nodes),
        format!("  GPUs:  {}/{} available", cluster.available_gpus, cluster.total_gpus),
        format!("  VRAM:  {}/{} GB", cluster.used_memory_gb, cluster.total_memory_gb),
        format!(""),
        format!("  Running:   {} workloads", cluster.running_workloads),
        format!("  Pending:   {} workloads", cluster.pending_workloads),
        format!("  Completed: {} workloads", cluster.completed_workloads),
        format!("  Failed:    {} workloads", cluster.failed_workloads),
    ];
    
    let text = stats.join("\n");
    let paragraph = Paragraph::new(text)
        .block(Block::default()
            .borders(Borders::ALL)
            .title(" Cluster Status ")
            .title_style(Style::default().fg(Color::Green)));
    
    frame.render_widget(paragraph, area);
}

fn draw_gpu_heatmap(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    
    if app.nodes.is_empty() {
        lines.push(Line::from("  No nodes connected"));
    } else {
        for node in &app.nodes {
            let mut spans = vec![
                Span::styled(
                    format!("  {:12} ", node.name.chars().take(12).collect::<String>()),
                    Style::default().fg(match node.status {
                        NodeStatus::Healthy => Color::Green,
                        NodeStatus::Unhealthy => Color::Red,
                        NodeStatus::Draining => Color::Yellow,
                        NodeStatus::Offline => Color::DarkGray,
                    }),
                ),
            ];
            
            // GPU blocks
            for gpu in &node.gpus {
                let color = utilization_color(gpu.utilization_percent);
                let block = if gpu.utilization_percent > 75 {
                    "▓▓"
                } else if gpu.utilization_percent > 25 {
                    "▓░"
                } else {
                    "░░"
                };
                spans.push(Span::styled(block, Style::default().fg(color)));
                spans.push(Span::raw(" "));
            }
            
            // Utilization percentage
            if !node.gpus.is_empty() {
                let avg_util: u32 = node.gpus.iter().map(|g| g.utilization_percent).sum::<u32>() 
                    / node.gpus.len() as u32;
                spans.push(Span::styled(
                    format!(" {:3}%", avg_util),
                    Style::default().fg(utilization_color(avg_util)),
                ));
            }
            
            lines.push(Line::from(spans));
        }
    }
    
    let paragraph = Paragraph::new(lines)
        .block(Block::default()
            .borders(Borders::ALL)
            .title(" GPU Heatmap ")
            .title_style(Style::default().fg(Color::Magenta)));
    
    frame.render_widget(paragraph, area);
}

fn draw_activity(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app.activity
        .iter()
        .skip(app.activity_scroll)
        .take(area.height.saturating_sub(2) as usize)
        .map(|item| {
            let time = chrono::DateTime::from_timestamp_millis(item.timestamp)
                .map(|dt| dt.format("%H:%M:%S").to_string())
                .unwrap_or_else(|| "??:??:??".to_string());
            
            let color = match item.event_type {
                ActivityType::Scale => Color::Cyan,
                ActivityType::Deploy => Color::Green,
                ActivityType::Preempt => Color::Yellow,
                ActivityType::NodeJoin => Color::Blue,
                ActivityType::NodeLeave => Color::Red,
                ActivityType::Trade => Color::Magenta,
                ActivityType::Alert => Color::Red,
                ActivityType::Info => Color::White,
            };
            
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", time), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{} ", item.event_type.symbol()), Style::default().fg(color)),
                Span::styled(&item.message, Style::default().fg(Color::White)),
            ]))
        })
        .collect();
    
    let list = List::new(items)
        .block(Block::default()
            .borders(Borders::ALL)
            .title(" Agent Activity ")
            .title_style(Style::default().fg(Color::Yellow)));
    
    frame.render_widget(list, area);
}

fn draw_workload_flow(frame: &mut Frame, app: &App, area: Rect) {
    let cluster = &app.cluster;
    
    let flow = format!(
        r#"
  pending ──┬──→ running ──┬──→ completed
    ({:3})   │     ({:3})    │      ({:4})
            │              │
            └──────────────┴──→ failed ({})
"#,
        cluster.pending_workloads,
        cluster.running_workloads,
        cluster.completed_workloads,
        cluster.failed_workloads,
    );
    
    let paragraph = Paragraph::new(flow)
        .block(Block::default()
            .borders(Borders::ALL)
            .title(" Workload Flow ")
            .title_style(Style::default().fg(Color::Blue)));
    
    frame.render_widget(paragraph, area);
}

fn draw_nodes(frame: &mut Frame, app: &App, area: Rect) {
    let header = Row::new(vec!["Node", "Status", "GPUs", "Util%", "Mem", "Temp", "Workloads"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    
    let rows: Vec<Row> = app.nodes.iter().map(|node| {
        let status_style = match node.status {
            NodeStatus::Healthy => Style::default().fg(Color::Green),
            NodeStatus::Unhealthy => Style::default().fg(Color::Red),
            NodeStatus::Draining => Style::default().fg(Color::Yellow),
            NodeStatus::Offline => Style::default().fg(Color::DarkGray),
        };
        
        let avg_util = if !node.gpus.is_empty() {
            node.gpus.iter().map(|g| g.utilization_percent).sum::<u32>() / node.gpus.len() as u32
        } else {
            0
        };
        
        let avg_temp = if !node.gpus.is_empty() {
            node.gpus.iter().map(|g| g.temperature_c).sum::<u32>() / node.gpus.len() as u32
        } else {
            0
        };
        
        let total_mem: u64 = node.gpus.iter().map(|g| g.memory_total_mb).sum();
        let used_mem: u64 = node.gpus.iter().map(|g| g.memory_used_mb).sum();
        
        Row::new(vec![
            node.name.clone(),
            format!("{:?}", node.status),
            format!("{}", node.gpus.len()),
            format!("{}%", avg_util),
            format!("{}/{}G", used_mem / 1024, total_mem / 1024),
            format!("{}°C", avg_temp),
            format!("{}", node.workload_count),
        ])
        .style(status_style)
    }).collect();
    
    let table = Table::new(rows, [
        Constraint::Length(15),
        Constraint::Length(10),
        Constraint::Length(6),
        Constraint::Length(6),
        Constraint::Length(12),
        Constraint::Length(6),
        Constraint::Length(10),
    ])
    .header(header)
    .block(Block::default()
        .borders(Borders::ALL)
        .title(" Cluster Nodes ")
        .title_style(Style::default().fg(Color::Cyan)));
    
    frame.render_widget(table, area);
}

fn draw_workloads(frame: &mut Frame, app: &App, area: Rect) {
    let header = Row::new(vec!["ID", "Name", "Image", "Status", "GPUs", "Node", "Progress"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    
    let rows: Vec<Row> = app.workloads.iter().map(|w| {
        let status_style = match w.state {
            WorkloadStatus::Running => Style::default().fg(Color::Green),
            WorkloadStatus::Pending => Style::default().fg(Color::Yellow),
            WorkloadStatus::Scheduling => Style::default().fg(Color::Cyan),
            WorkloadStatus::Completed => Style::default().fg(Color::Blue),
            WorkloadStatus::Failed => Style::default().fg(Color::Red),
        };
        
        Row::new(vec![
            w.id.chars().take(8).collect::<String>(),
            w.name.clone().unwrap_or_else(|| "-".to_string()),
            w.image.chars().take(20).collect::<String>(),
            format!("{:?}", w.state),
            format!("{}", w.gpu_count),
            w.assigned_node.clone().unwrap_or_else(|| "-".to_string()),
            w.progress_percent.map(|p| format!("{}%", p)).unwrap_or_else(|| "-".to_string()),
        ])
        .style(status_style)
    }).collect();
    
    let table = Table::new(rows, [
        Constraint::Length(10),
        Constraint::Length(15),
        Constraint::Length(22),
        Constraint::Length(12),
        Constraint::Length(6),
        Constraint::Length(12),
        Constraint::Length(10),
    ])
    .header(header)
    .block(Block::default()
        .borders(Borders::ALL)
        .title(" Workloads ")
        .title_style(Style::default().fg(Color::Green)));
    
    frame.render_widget(table, area);
}

fn draw_market(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    
    // Spot prices
    let price_items: Vec<ListItem> = app.market.spot_prices.iter().map(|p| {
        let change_color = if p.change_percent > 0.0 {
            Color::Green
        } else if p.change_percent < 0.0 {
            Color::Red
        } else {
            Color::White
        };
        
        let change_symbol = if p.change_percent > 0.0 { "▲" } else if p.change_percent < 0.0 { "▼" } else { "─" };
        
        ListItem::new(Line::from(vec![
            Span::styled(format!("  {:8} ", p.gpu_model), Style::default().fg(Color::Cyan)),
            Span::styled(format!("${:.2}/hr ", p.price_per_hour), Style::default().fg(Color::Yellow)),
            Span::styled(format!("{} {:+.1}% ", change_symbol, p.change_percent), Style::default().fg(change_color)),
            Span::styled(format!("({} avail)", p.available_count), Style::default().fg(Color::DarkGray)),
        ]))
    }).collect();
    
    let prices_list = List::new(price_items)
        .block(Block::default()
            .borders(Borders::ALL)
            .title(" MOLT Spot Prices ")
            .title_style(Style::default().fg(Color::Magenta)));
    
    frame.render_widget(prices_list, chunks[0]);
    
    // Recent trades
    let trade_items: Vec<ListItem> = app.market.recent_trades.iter().map(|t| {
        let time = chrono::DateTime::from_timestamp_millis(t.timestamp)
            .map(|dt| dt.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "??:??:??".to_string());
        
        ListItem::new(Line::from(vec![
            Span::styled(format!("{} ", time), Style::default().fg(Color::DarkGray)),
            Span::styled("💰 ", Style::default()),
            Span::styled(format!("{}x {} ", t.gpu_count, t.gpu_model), Style::default().fg(Color::Cyan)),
            Span::styled(format!("@ ${:.2}/hr ", t.price_per_hour), Style::default().fg(Color::Yellow)),
            Span::styled(format!("for {}h", t.duration_hours), Style::default().fg(Color::White)),
        ]))
    }).collect();
    
    let trades_list = List::new(trade_items)
        .block(Block::default()
            .borders(Borders::ALL)
            .title(format!(" Recent Trades (Offers: {} | Bids: {}) ", 
                app.market.active_offers, app.market.active_bids))
            .title_style(Style::default().fg(Color::Green)));
    
    frame.render_widget(trades_list, chunks[1]);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let status = if app.connected {
        Span::styled("● Connected", Style::default().fg(Color::Green))
    } else {
        Span::styled("○ Disconnected", Style::default().fg(Color::Red))
    };
    
    let last_update = app.last_update
        .and_then(|ts| chrono::DateTime::from_timestamp_millis(ts))
        .map(|dt| dt.format("Last update: %H:%M:%S").to_string())
        .unwrap_or_else(|| "No updates".to_string());
    
    let help = "  [Tab] Switch view  [↑↓] Scroll  [q] Quit  ";
    
    let footer = Paragraph::new(Line::from(vec![
        Span::raw("  "),
        status,
        Span::raw("  │  "),
        Span::styled(last_update, Style::default().fg(Color::DarkGray)),
        Span::raw("  │  "),
        Span::styled(help, Style::default().fg(Color::DarkGray)),
    ]))
    .block(Block::default().borders(Borders::ALL));
    
    frame.render_widget(footer, area);
}

fn utilization_color(percent: u32) -> Color {
    if percent >= 90 {
        Color::Red
    } else if percent >= 70 {
        Color::Yellow
    } else if percent >= 40 {
        Color::Green
    } else {
        Color::Blue
    }
}
