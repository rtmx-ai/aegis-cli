//! Mock audit ledger for testing.

use aegis_domain::error::DomainError;
use aegis_domain::event::DomainEvent;
use aegis_domain::ports::AuditLedger;
use async_trait::async_trait;
use std::sync::Mutex;

/// A mock ledger that collects events in memory for assertion.
pub struct MockAuditLedger {
    events: Mutex<Vec<DomainEvent>>,
}

impl MockAuditLedger {
    pub fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }

    /// Get all recorded events for assertion.
    pub fn events(&self) -> Vec<DomainEvent> {
        self.events.lock().unwrap().clone()
    }

    /// Get the number of recorded events.
    pub fn event_count(&self) -> usize {
        self.events.lock().unwrap().len()
    }
}

#[async_trait]
impl AuditLedger for MockAuditLedger {
    async fn record(&self, event: &DomainEvent) -> Result<(), DomainError> {
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }
}
