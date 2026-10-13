#[cfg(not(target_arch = "wasm32"))]
mod backend;
#[cfg(not(target_arch = "wasm32"))]
pub use backend::*;
mod api_stats;
#[cfg(not(target_arch = "wasm32"))]
mod bidi;
#[cfg(not(target_arch = "wasm32"))]
mod bidi_transport;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod bidi_transport_props;
mod boxed_streams;
mod builder;
pub mod stream;

pub use api_stats::*;
#[cfg(not(target_arch = "wasm32"))]
pub use bidi::*;
#[cfg(not(target_arch = "wasm32"))]
pub use bidi_transport::*;
pub use boxed_streams::*;
pub use builder::*;

/// Keep input order while splitting at either the item count or byte budget.
/// An item that alone exceeds a budget gets its own chunk; the caller rejects it.
pub(crate) fn chunk_by_budget<T>(
    items: Vec<T>,
    max_count: usize,
    max_bytes: usize,
    wire_cost: impl Fn(&T) -> usize,
) -> Vec<Vec<T>> {
    let mut chunks: Vec<Vec<T>> = Vec::new();
    let mut current: Vec<T> = Vec::new();
    let mut current_bytes = 0usize;
    for item in items {
        let cost = wire_cost(&item);
        if !current.is_empty() && (current.len() >= max_count || current_bytes + cost > max_bytes) {
            chunks.push(std::mem::take(&mut current));
            current_bytes = 0;
        }
        current_bytes += cost;
        current.push(item);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}
