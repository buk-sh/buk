use anyhow::Result;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

pub struct FileWatcher {
    watcher: RecommendedWatcher,
    rx: Receiver<Result<Event, notify::Error>>,
}

impl FileWatcher {
    pub fn new(_paths: &[&str]) -> Result<Self> {
        let (tx, rx) = channel();
        
        let watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default().with_poll_interval(Duration::from_millis(500)),
        )?;
        
        Ok(Self { watcher, rx })
    }
    
    pub fn watch(&mut self, path: &str) -> Result<()> {
        self.watcher.watch(Path::new(path), RecursiveMode::Recursive)?;
        Ok(())
    }
    
    pub fn unwatch(&mut self, path: &str) -> Result<()> {
        self.watcher.unwatch(Path::new(path))?;
        Ok(())
    }
    
    pub fn check_changed(&self) -> bool {
        // Non-blocking check for changes
        match self.rx.try_recv() {
            Ok(Ok(_)) => true,
            _ => false,
        }
    }
    
    pub fn wait_for_change(&self) -> Option<Event> {
        // Blocking wait for changes
        match self.rx.recv() {
            Ok(Ok(event)) => Some(event),
            _ => None,
        }
    }
}

pub fn should_restart(path: &str) -> bool {
    path.ends_with(".js") || path.ends_with(".ts") || path.ends_with(".json")
}
