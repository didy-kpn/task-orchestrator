use std::{collections::HashMap, hash::Hash, sync::Arc};

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::{Mutex, broadcast};
use uuid::Uuid;

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
    #[error("Task not found: {0}")]
    TaskNotFound(TaskId),
}

pub trait EventType: Send + Sync + Clone + Hash + Eq {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId(Uuid);

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[async_trait]
pub trait Executable: Send + Sync {
    fn name(&self) -> &str;

    async fn initialize(&mut self) -> Result<(), SchedulerError>;
    async fn shutdown(&mut self) -> Result<(), SchedulerError>;
}

#[async_trait]
pub trait BackgroundTask: Executable {
    async fn execute(&mut self) -> Result<(), SchedulerError>;
}

#[async_trait]
pub trait EventTask<E: EventType + 'static>: Executable {
    fn subscribed_event(&self) -> &E;
    async fn handle_event(&mut self, event: String) -> Result<(), SchedulerError>;
}

#[derive(Debug, Clone)]
pub struct ChannelConfig {
    pub capacity: usize,
    pub description: String,
}

pub struct EventBus<E> {
    channels: HashMap<E, broadcast::Sender<String>>,
    configs: Vec<ChannelConfig>,
}

impl<E: EventType + 'static + ToString> EventBus<E> {
    pub fn new(configs: Vec<(E, ChannelConfig)>) -> Self {
        Self {
            channels: configs
                .iter()
                .map(|c| (c.0.clone(), broadcast::channel(c.1.capacity).0))
                .collect(),
            configs: configs.iter().map(|c| c.1.clone()).collect(),
        }
    }

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

    pub fn clone_sender(
        &self,
        channel_event: &E,
    ) -> Result<broadcast::Sender<String>, SchedulerError> {
        self.channels
            .get(channel_event)
            .ok_or(SchedulerError::InvalidChannel(channel_event.to_string()))
            .cloned()
    }

    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    pub fn channel_config(&self, channel_idx: usize) -> Option<&ChannelConfig> {
        self.configs.get(channel_idx)
    }
}

pub struct TaskRegistry<E> {
    background_tasks: HashMap<TaskId, Arc<Mutex<dyn BackgroundTask>>>,
    event_tasks: HashMap<TaskId, Arc<Mutex<dyn EventTask<E>>>>,
}

impl<E: EventType + 'static + ToString> Default for TaskRegistry<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: EventType + 'static + ToString> TaskRegistry<E> {
    pub fn new() -> Self {
        Self {
            background_tasks: HashMap::new(),
            event_tasks: HashMap::new(),
        }
    }

    pub fn register_background_task(&mut self, task: Arc<Mutex<dyn BackgroundTask>>) -> TaskId {
        let id = TaskId::new();
        self.background_tasks.insert(id.clone(), task);
        id
    }

    pub fn register_event_task(&mut self, task: Arc<Mutex<dyn EventTask<E>>>) -> TaskId {
        let id = TaskId::new();
        self.event_tasks.insert(id.clone(), task);
        id
    }

    pub fn get_background_task(&self, id: &TaskId) -> Option<Arc<Mutex<dyn BackgroundTask>>> {
        self.background_tasks.get(id).cloned()
    }

    pub fn get_event_task(&self, id: &TaskId) -> Option<Arc<Mutex<dyn EventTask<E>>>> {
        self.event_tasks.get(id).cloned()
    }
}

pub struct Scheduler<E> {
    task_registry: TaskRegistry<E>,
    background_task_ids: Vec<TaskId>,
    event_task_ids: Vec<TaskId>,
    event_bus: EventBus<E>,
}

impl<E: EventType + 'static + ToString> Scheduler<E> {
    pub fn new(event_bus: EventBus<E>) -> Self {
        Self {
            task_registry: TaskRegistry::new(),
            background_task_ids: Vec::new(),
            event_task_ids: Vec::new(),
            event_bus,
        }
    }

    pub fn event_bus(&self) -> &EventBus<E> {
        &self.event_bus
    }

    pub fn register_background_task(&mut self, task: Arc<Mutex<dyn BackgroundTask>>) -> TaskId {
        let id = self.task_registry.register_background_task(task);
        self.background_task_ids.push(id.clone());
        id
    }

    pub fn register_event_task(&mut self, task: Arc<Mutex<dyn EventTask<E>>>) -> TaskId {
        let id = self.task_registry.register_event_task(task);
        self.event_task_ids.push(id.clone());
        id
    }

    pub async fn start(&mut self) -> Result<(), SchedulerError> {
        for id in &self.background_task_ids {
            let task = self
                .task_registry
                .get_background_task(id)
                .ok_or_else(|| SchedulerError::TaskNotFound(id.clone()))?;

            task.lock().await.initialize().await?;

            let task_clone = task.clone();
            tokio::spawn(async move {
                loop {
                    if let Err(e) = task_clone.lock().await.execute().await {
                        eprintln!("Background task execution error: {}", e);
                    }
                }
            });
        }

        for id in &self.event_task_ids {
            let task = self
                .task_registry
                .get_event_task(id)
                .ok_or_else(|| SchedulerError::TaskNotFound(id.clone()))?;

            let mut task_lock = task.lock().await;
            task_lock.initialize().await?;
            let event = task_lock.subscribed_event().clone();
            drop(task_lock);

            if let Ok(mut rx) = self.event_bus.subscribe(&event) {
                let task_clone = task.clone();
                tokio::spawn(async move {
                    while let Ok(event_data) = rx.recv().await {
                        if let Err(e) = task_clone.lock().await.handle_event(event_data).await {
                            eprintln!("Event handling error: {}", e);
                        }
                    }
                });
            }
        }

        Ok(())
    }

    pub async fn shutdown(&mut self) -> Result<(), SchedulerError> {
        for id in &self.background_task_ids {
            if let Some(task) = self.task_registry.get_background_task(id) {
                task.lock().await.shutdown().await?;
            }
        }

        for id in &self.event_task_ids {
            if let Some(task) = self.task_registry.get_event_task(id) {
                task.lock().await.shutdown().await?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[derive(Debug, Clone, Hash, PartialEq, Eq)]
    enum TestEvent {
        EventA,
        EventB,
    }

    impl std::fmt::Display for TestEvent {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                TestEvent::EventA => write!(f, "EventA"),
                TestEvent::EventB => write!(f, "EventB"),
            }
        }
    }

    impl EventType for TestEvent {}

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TestStatus {
        Idle,
        Running,
        Stopped,
    }

    struct BackgroundTestTask {
        name: String,
        status: TestStatus,
        status_inner: Arc<Mutex<TestStatus>>,
    }

    #[async_trait]
    impl Executable for BackgroundTestTask {
        fn name(&self) -> &str {
            &self.name
        }

        async fn initialize(&mut self) -> Result<(), SchedulerError> {
            *self.status_inner.lock().await = TestStatus::Running;
            self.status = TestStatus::Running;
            Ok(())
        }

        async fn shutdown(&mut self) -> Result<(), SchedulerError> {
            *self.status_inner.lock().await = TestStatus::Stopped;
            self.status = TestStatus::Stopped;
            Ok(())
        }
    }

    #[async_trait]
    impl BackgroundTask for BackgroundTestTask {
        async fn execute(&mut self) -> Result<(), SchedulerError> {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            Ok(())
        }
    }

    struct EventTestTask {
        name: String,
        status: TestStatus,
        status_inner: Arc<Mutex<TestStatus>>,
        event: TestEvent,
        received_events: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Executable for EventTestTask {
        fn name(&self) -> &str {
            &self.name
        }

        async fn initialize(&mut self) -> Result<(), SchedulerError> {
            *self.status_inner.lock().await = TestStatus::Running;
            self.status = TestStatus::Running;
            Ok(())
        }

        async fn shutdown(&mut self) -> Result<(), SchedulerError> {
            *self.status_inner.lock().await = TestStatus::Stopped;
            self.status = TestStatus::Stopped;
            Ok(())
        }
    }

    #[async_trait]
    impl EventTask<TestEvent> for EventTestTask {
        fn subscribed_event(&self) -> &TestEvent {
            &self.event
        }

        async fn handle_event(&mut self, event: String) -> Result<(), SchedulerError> {
            self.received_events.lock().await.push(event);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_event_bus_creation() {
        let event_bus = EventBus::<TestEvent>::new(vec![
            (
                TestEvent::EventA,
                ChannelConfig {
                    capacity: 10,
                    description: "Channel A".to_string(),
                },
            ),
            (
                TestEvent::EventB,
                ChannelConfig {
                    capacity: 20,
                    description: "Channel B".to_string(),
                },
            ),
        ]);

        assert_eq!(event_bus.channel_count(), 2);
        assert!(event_bus.clone_sender(&TestEvent::EventA).is_ok());
        assert!(event_bus.clone_sender(&TestEvent::EventB).is_ok());
    }

    #[tokio::test]
    async fn test_task_registration() {
        let mut registry = TaskRegistry::<TestEvent>::new();

        let bg_task = Arc::new(Mutex::new(BackgroundTestTask {
            name: "bg_task".to_string(),
            status: TestStatus::Idle,
            status_inner: Arc::new(Mutex::new(TestStatus::Idle)),
        }));

        let event_task = Arc::new(Mutex::new(EventTestTask {
            name: "event_task".to_string(),
            status: TestStatus::Idle,
            status_inner: Arc::new(Mutex::new(TestStatus::Idle)),
            event: TestEvent::EventA,
            received_events: Arc::new(Mutex::new(Vec::new())),
        }));

        let bg_id = registry.register_background_task(bg_task.clone());
        let event_id = registry.register_event_task(event_task.clone());

        assert!(registry.get_background_task(&bg_id).is_some());
        assert!(registry.get_event_task(&event_id).is_some());
    }

    #[tokio::test]
    async fn test_background_task_lifecycle() {
        let status = Arc::new(Mutex::new(TestStatus::Idle));
        let task = Arc::new(Mutex::new(BackgroundTestTask {
            name: "bg_task".to_string(),
            status: TestStatus::Idle,
            status_inner: status.clone(),
        }));

        let mut scheduler = Scheduler::new(EventBus::<TestEvent>::new(vec![]));
        scheduler.register_background_task(task.clone());

        // Test initialization
        scheduler.start().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(*status.lock().await, TestStatus::Running);

        // Test shutdown
        scheduler.shutdown().await.unwrap();
        assert_eq!(*status.lock().await, TestStatus::Stopped);
    }

    #[tokio::test]
    async fn test_event_task_handling() {
        let event_bus = EventBus::<TestEvent>::new(vec![(
            TestEvent::EventA,
            ChannelConfig {
                capacity: 10,
                description: "Test channel".to_string(),
            },
        )]);

        let status = Arc::new(Mutex::new(TestStatus::Idle));
        let received_events = Arc::new(Mutex::new(Vec::new()));
        let task = Arc::new(Mutex::new(EventTestTask {
            name: "event_task".to_string(),
            status: TestStatus::Idle,
            status_inner: status.clone(),
            event: TestEvent::EventA,
            received_events: received_events.clone(),
        }));

        let mut scheduler = Scheduler::new(event_bus);
        scheduler.register_event_task(task.clone());
        scheduler.start().await.unwrap();

        // Send multiple events
        let sender = scheduler
            .event_bus()
            .clone_sender(&TestEvent::EventA)
            .unwrap();
        for i in 0..5 {
            sender.send(format!("event_{}", i)).unwrap();
        }

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let events = received_events.lock().await;
        assert_eq!(events.len(), 5);
        for i in 0..5 {
            assert_eq!(events[i], format!("event_{}", i));
        }
    }

    #[tokio::test]
    async fn test_scheduler_error_handling() {
        let scheduler = Scheduler::new(EventBus::<TestEvent>::new(vec![]));

        // Test invalid task lookup
        let result = scheduler
            .task_registry
            .get_background_task(&TaskId::new())
            .is_none();
        assert!(result);

        // Test invalid channel
        let event_bus = EventBus::<TestEvent>::new(vec![]);
        assert!(event_bus.clone_sender(&TestEvent::EventA).is_err());
    }
}
