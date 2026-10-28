//! Chart tile: candles over the most recent bars.
use serde::{Deserialize, Serialize};

/// Per-tile chart options (saved with the layout).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChartOpts {}
