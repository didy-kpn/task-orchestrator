use std::{collections::HashMap, hash::Hash};

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc};

#[derive(Error, Debug)]
pub enum SchedulerError {
    #[error("Task initialization error: {0}")]
    TaskInitialization(String),
    #[error("Task execution error: {0}")]
    TaskExecution(String),
    #[error("Task shutdown error: {0}")]
    TaskShutdown(String),
    #[error("Invalid channel: {0}")]
    InvalidChannel(String),
    #[error("Event send error: {0}")]
    EventSend(String),
    #[error("Event receive error: {0}")]
    EventReceive(String),
}

pub trait EventType: Send + Sync + Clone + std::fmt::Debug + Hash + Eq + std::fmt::Display {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Background,
    EventDriven,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStatus {
    Idle,
    Running,
    Stopped,
    Failed,
}

#[async_trait]
pub trait Executable<E: EventType + 'static>: Send + Sync {
    fn name(&self) -> &str;
    fn mode(&self) -> ExecutionMode;
    fn status(&self) -> ExecutionStatus;

    fn clone_box(&self) -> Box<dyn Executable<E>>;

    async fn initialize(&mut self) -> Result<(), SchedulerError>;
    async fn execute(&mut self) -> Result<(), SchedulerError>;
    async fn shutdown(&mut self) -> Result<(), SchedulerError>;

    fn subscribed_event(&self) -> &E;
    async fn handle_event(&mut self, event: String) -> Result<(), SchedulerError>;

    fn message_sender(&self) -> Option<mpsc::Sender<String>>;
}

#[derive(Debug, Clone)]
pub struct ChannelConfig {
    pub capacity: usize,
    pub description: String,
}

#[derive(Default)]
pub struct EventBus<E> {
    channels: HashMap<E, broadcast::Sender<String>>,
    configs: Vec<ChannelConfig>,
}

impl<E: EventType + 'static> EventBus<E> {
    pub fn add_channel(mut self, event: E, config: ChannelConfig) -> Self {
        self.channels
            .insert(event, broadcast::channel(config.capacity).0);
        self.configs.push(config);
        self
    }

    pub fn subscribe(
        &self,
        channel_event: &E,
    ) -> Result<broadcast::Receiver<String>, SchedulerError> {
        self.channels
            .get(channel_event)
            .ok_or(SchedulerError::InvalidChannel(channel_event.to_string()))
            .map(|sender| sender.subscribe())
    }

    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    pub fn channel_config(&self, channel_idx: usize) -> Option<&ChannelConfig> {
        self.configs.get(channel_idx)
    }
}

pub struct Scheduler<E> {
    tasks: Vec<Box<dyn Executable<E>>>,
    event_bus: EventBus<E>,
}

impl<E: EventType + 'static> Scheduler<E> {
    pub fn new(event_bus: EventBus<E>) -> Self {
        Self {
            tasks: Vec::new(),
            event_bus,
        }
    }

    pub fn register_task(&mut self, task: Box<dyn Executable<E>>) {
        self.tasks.push(task);
    }

    pub async fn start(&mut self) -> Result<(), SchedulerError> {
        for task in self.tasks.iter_mut() {
            task.initialize().await?;

            match task.mode() {
                ExecutionMode::Background => {
                    let mut task_clone = task.clone_box();
                    tokio::spawn(async move {
                        loop {
                            if let Err(e) = task_clone.execute().await {
                                eprintln!("Task execution error: {}", e);
                            }
                        }
                    });
                }
                ExecutionMode::EventDriven => {
                    if let Ok(mut rx) = self.event_bus.subscribe(task.subscribed_event()) {
                        let mut task_clone = task.clone_box();

                        tokio::spawn(async move {
                            while let Ok(event) = rx.recv().await {
                                if let Err(e) = task_clone.handle_event(event).await {
                                    eprintln!("Event handling error: {}", e);
                                }
                            }
                        });
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn shutdown(&mut self) -> Result<(), SchedulerError> {
        for task in self.tasks.iter_mut() {
            task.shutdown().await?;
        }
        Ok(())
    }
}
