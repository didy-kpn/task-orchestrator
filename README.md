# Task Orchestrator 🚀

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A high-performance, type-safe asynchronous task scheduling library with event-driven architecture.

## Features ✨

- 🚦 **Event-driven task scheduling**
- 📡 **Layered event queues** with configurable channels
- ⚡ **Asynchronous execution** powered by Tokio
- 🔒 **Type-safe event system** with compile-time guarantees
- 🔄 **Task lifecycle management** (initialize, execute, shutdown)
- 📊 **Execution modes**: Background and Event-driven tasks
- 🧩 **Extensible architecture** with trait-based design

## Installation 📦

Add this to your `Cargo.toml`:

```toml
[dependencies]
task-orchestrator = "0.1.0"
tokio = { version = "1.0", features = ["full"] }
```

## Quick Start 🚀

```rust
use task_orchestrator::{EventBus, Scheduler, EventType, Executable, ExecutionMode};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
enum MyEvent {
    ProcessData,
    ShutdownSignal,
}

impl std::fmt::Display for MyEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl EventType for MyEvent {}

struct MyTask {
    // Your task implementation
}

#[async_trait::async_trait]
impl Executable<MyEvent> for MyTask {
    // Implement required methods
}

#[tokio::main]
async fn main() {
    let event_bus = EventBus::new(vec![
        (MyEvent::ProcessData, ChannelConfig::default()),
        (MyEvent::ShutdownSignal, ChannelConfig::default()),
    ]);
    
    let mut scheduler = Scheduler::new(event_bus);
    
    // Register tasks
    // scheduler.register_task(...);
    
    // Start the scheduler
    scheduler.start().await.unwrap();
}
```

## License 📄

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
