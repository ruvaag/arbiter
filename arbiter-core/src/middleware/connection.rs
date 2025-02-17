//! Messengers/connections to the underlying EVM in the environment.
use std::{
    collections::HashMap,
    fmt::Debug,
    sync::{Arc, Mutex, Weak},
};

use ethers::{
    prelude::ProviderError,
    providers::JsonRpcClient,
    types::{Filter, FilteredParams},
};
use serde::{de::DeserializeOwned, Serialize};

use super::cast::revm_logs_to_ethers_logs;
use crate::environment::{EventBroadcaster, InstructionSender, OutcomeReceiver, OutcomeSender};

/// Represents a connection to the EVM contained in the corresponding
/// [`Environment`].
#[derive(Debug)]
pub struct Connection {
    /// Used to send calls and transactions to the [`Environment`] to be
    /// executed by `revm`.
    pub(crate) instruction_sender: Weak<InstructionSender>,

    /// Used to send results back to a client that made a call/transaction with
    /// the [`Environment`]. This [`ResultSender`] is passed along with a
    /// call/transaction so the [`Environment`] can reply back with the
    /// [`ExecutionResult`].
    pub(crate) outcome_sender: OutcomeSender,

    /// Used to receive the [`ExecutionResult`] from the [`Environment`] upon
    /// call/transact.
    pub(crate) outcome_receiver: OutcomeReceiver,

    /// A reference to the [`EventBroadcaster`] so that more receivers of the
    /// broadcast can be taken from it.
    pub(crate) event_broadcaster: Arc<Mutex<EventBroadcaster>>,

    /// A collection of `FilterReceiver`s that will receive outgoing logs
    /// generated by `revm` and output by the [`Environment`].
    pub(crate) filter_receivers:
        Arc<tokio::sync::Mutex<HashMap<ethers::types::U256, FilterReceiver>>>,
}

#[async_trait::async_trait]
impl JsonRpcClient for Connection {
    type Error = ProviderError;

    /// Processes a JSON-RPC request and returns the response.
    /// Currently only handles the `eth_getFilterChanges` call since this is
    /// used for polling events emitted from the [`Environment`].
    async fn request<T: Serialize + Send + Sync, R: DeserializeOwned>(
        &self,
        method: &str,
        params: T,
    ) -> Result<R, ProviderError> {
        match method {
            "eth_getFilterChanges" => {
                // TODO: The extra json serialization/deserialization can probably be avoided
                // somehow

                // Get the `Filter` ID from the params `T`
                // First convert it into a JSON `Value`
                let value = serde_json::to_value(&params)?;

                // Take this value as an array then cast it to a string
                let str = value.as_array().ok_or(ProviderError::CustomError(
                    "The params value passed to the `Connection` via a `request` was empty. 
                    This is likely due to not specifying a specific `Filter` ID!".to_string()
                ))?[0]
                    .as_str().ok_or(ProviderError::CustomError(
                        "The params value passed to the `Connection` via a `request` could not be later cast to `str`!".to_string()
                    ))?;

                // Now get the `U256` ID via the string decoded from hex radix.
                let id = ethers::types::U256::from_str_radix(str, 16)
                    .map_err(|e| ProviderError::CustomError(
                        format!("The `str` representation of the filter ID could not be cast into `U256` due to: {:?}!", 
                        e)))?;

                // Get the corresponding `filter_receiver` and await for logs to appear.
                let mut filter_receivers = self.filter_receivers.lock().await;
                let filter_receiver =
                    filter_receivers
                        .get_mut(&id)
                        .ok_or(ProviderError::CustomError(
                            "The filter ID does not seem to match any that this client owns!"
                                .to_string(),
                        ))?;
                let mut logs = vec![];
                let filtered_params = FilteredParams::new(Some(filter_receiver.filter.clone()));
                if let Ok(received_logs) = filter_receiver.receiver.try_recv() {
                    let ethers_logs = revm_logs_to_ethers_logs(received_logs);
                    for log in ethers_logs {
                        if filtered_params.filter_address(&log)
                            && filtered_params.filter_topics(&log)
                        {
                            logs.push(log);
                        }
                    }
                }
                // Take the logs and Stringify then JSONify to cast into `R`.
                let logs_str = serde_json::to_string(&logs)?;
                let logs_deserializeowned: R = serde_json::from_str(&logs_str)?;
                Ok(logs_deserializeowned)
            }
            _ => Err(ProviderError::UnsupportedRPC),
        }
    }
}

/// Packages together a [`crossbeam_channel::Receiver<Vec<Log>>`] along with a
/// [`Filter`] for events. Allows the client to have a stream of filtered
/// events.
#[derive(Debug)]
pub(crate) struct FilterReceiver {
    /// The filter definition used for this receiver.
    /// Comes from the `ethers-rs` crate.
    pub(crate) filter: Filter,

    /// The receiver for the channel that receives logs from the broadcaster.
    /// These are filtered upon reception.
    pub(crate) receiver: crossbeam_channel::Receiver<Vec<revm::primitives::Log>>,
}
