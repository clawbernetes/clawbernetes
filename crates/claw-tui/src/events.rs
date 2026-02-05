//! Event handling for Clawbernetes TUI

use std::time::Duration;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;

/// Application events
#[derive(Debug)]
pub enum AppEvent {
    /// Terminal key press
    Key(KeyEvent),
    /// Terminal resize
    Resize(u16, u16),
    /// Tick for periodic updates
    Tick,
    /// Data update from backend
    DataUpdate(DataEvent),
}

/// Data events from the cluster
#[derive(Debug, Clone)]
pub enum DataEvent {
    ClusterUpdate(serde_json::Value),
    NodeUpdate(serde_json::Value),
    WorkloadUpdate(serde_json::Value),
    GpuMetrics(serde_json::Value),
    Activity(serde_json::Value),
    MarketUpdate(serde_json::Value),
    Connected,
    Disconnected,
    Error(String),
}

/// Event handler that polls for terminal events
pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<AppEvent>,
    _tx: mpsc::UnboundedSender<AppEvent>,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let event_tx = tx.clone();
        
        // Spawn terminal event handler
        tokio::spawn(async move {
            loop {
                if event::poll(tick_rate).unwrap_or(false) {
                    match event::read() {
                        Ok(Event::Key(key)) => {
                            if event_tx.send(AppEvent::Key(key)).is_err() {
                                break;
                            }
                        }
                        Ok(Event::Resize(w, h)) => {
                            if event_tx.send(AppEvent::Resize(w, h)).is_err() {
                                break;
                            }
                        }
                        _ => {}
                    }
                } else {
                    if event_tx.send(AppEvent::Tick).is_err() {
                        break;
                    }
                }
            }
        });
        
        Self { rx, _tx: tx }
    }
    
    pub async fn next(&mut self) -> Option<AppEvent> {
        self.rx.recv().await
    }
    
    pub fn sender(&self) -> mpsc::UnboundedSender<AppEvent> {
        self._tx.clone()
    }
}

/// Handle keyboard input
pub fn handle_key(app: &mut crate::app::App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') => {
            app.running = false;
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.running = false;
        }
        KeyCode::Tab => {
            app.next_tab();
        }
        KeyCode::BackTab => {
            app.prev_tab();
        }
        KeyCode::Left => {
            app.prev_tab();
        }
        KeyCode::Right => {
            app.next_tab();
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.scroll_activity_up();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.scroll_activity_down();
        }
        KeyCode::Char('1') => {
            app.selected_tab = 0;
        }
        KeyCode::Char('2') => {
            app.selected_tab = 1;
        }
        KeyCode::Char('3') => {
            app.selected_tab = 2;
        }
        KeyCode::Char('4') => {
            app.selected_tab = 3;
        }
        _ => {}
    }
}
