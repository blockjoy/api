//! Build the event system

use std::fmt::Display;
use std::sync::Arc;
use crate::errors::ApiError;

#[derive(Debug, PartialEq, Eq)]
pub enum EventTypes {
    NewCommand,
    UpdateCommand,
}

pub trait IntoPayload: Display {
    fn into_payload(&self) -> Self
        where Self: Sized;
}

pub struct EventDispatcher {
    handlers: Vec<(EventTypes, Box<dyn FnOnce(Arc<dyn IntoPayload>) -> Result<(), ApiError>>)>
}

impl EventDispatcher {
    pub fn new() -> Self {
        Self { handlers: vec![] }
    }

    /// Register event handler to defined event
    pub fn register<F>(&mut self, evt: EventTypes, handler: F) -> Result<(), ApiError>
        where F: Fn(Arc<dyn IntoPayload>) -> Result<(), ApiError> + 'static
    {
        self.handlers.push((evt, Box::new(handler)));

        Ok(())
    }

    /// Notify registered handlers about a specific event
    pub async fn notify(self, evt: EventTypes, pl: Arc<dyn IntoPayload>) {
        for pair in self.handlers {
            if pair.0 == evt {
                let func = pair.1;

                match func(pl.clone()) {
                    Err(e) => tracing::error!("Error calling event handler: {:?}", e),
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;
    use crate::errors::ApiError;
    use crate::events::{EventDispatcher, EventTypes, IntoPayload};

    impl IntoPayload for String {
        fn into_payload(&self) -> Self {
            self.clone()
        }
    }

    fn test_handler(pl: Arc<impl IntoPayload>) -> Result<(), ApiError> {
        println!("some handler with payload: {}", pl.into_payload());
        Ok(())
    }

    #[test]
    fn can_create_payload() {
        let pl = String::from("the payload");

        assert_eq!(pl.into_payload(), "the payload".to_string())
    }

    #[test]
    fn can_create_dispatcher() {
        EventDispatcher::new();
    }

    #[test]
    fn can_register_handler() {
        let mut dispatcher = EventDispatcher::new();

        dispatcher.register(EventTypes::NewCommand, test_handler).expect("register didn't work");
    }

    /*
    #[test]
    fn can_notify_handlers() {
        let mut dispatcher = EventDispatcher::new();

        dispatcher.register(EventTypes::NewCommand, test_handler).expect("register didn't work");
        dispatcher.notify(EventTypes::NewCommand, Arc::new("asdasdf".to_string()));
    }

     */
}