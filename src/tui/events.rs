use std::time::Duration;
use tokio::sync::mpsc;
use zeroclaw_runtime::agent::tui_events::RuntimeEvent;

#[derive(Debug)]
pub enum TuiAppEvent {
    Key(crossterm::event::KeyEvent),
    Resize(u16, u16),
    Runtime(RuntimeEvent),
    Tick,
}

pub fn start_event_loop(
    mut runtime_rx: mpsc::Receiver<RuntimeEvent>,
) -> mpsc::Receiver<TuiAppEvent> {
    let (tx, rx) = mpsc::channel(100);

    // 1. Stdin Key/Resize listener
    let tx_keys = tx.clone();
    tokio::spawn(async move {
        loop {
            if let Ok(true) = crossterm::event::poll(Duration::from_millis(50)) {
                match crossterm::event::read() {
                    Ok(crossterm::event::Event::Key(key)) => {
                        let _ = tx_keys.send(TuiAppEvent::Key(key)).await;
                    }
                    Ok(crossterm::event::Event::Resize(w, h)) => {
                        let _ = tx_keys.send(TuiAppEvent::Resize(w, h)).await;
                    }
                    _ => {}
                }
            }
        }
    });

    // 2. Runtime Event Listener
    let tx_runtime = tx.clone();
    tokio::spawn(async move {
        while let Some(event) = runtime_rx.recv().await {
            let _ = tx_runtime.send(TuiAppEvent::Runtime(event)).await;
        }
    });

    // 3. Tick Listener (for spinner animation)
    let tx_tick = tx;
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(150)).await;
            if tx_tick.send(TuiAppEvent::Tick).await.is_err() {
                break;
            }
        }
    });

    rx
}
