//! Pure capability policy. Callers supply authenticated, coherent current facts.

mod catalog;
mod delegation;
mod evaluate;
mod grants;
mod model;

pub use catalog::{Catalog, CatalogError, DefinitionKind};
pub use delegation::{CapabilitySelection, IssuancePlan, attenuate, plan_key, plan_oauth};
pub use evaluate::{Denial, Evaluation, authorize, effective_capabilities};
pub use model::*;

#[cfg(test)]
#[path = "../../tests/unit/authorization/fixtures.rs"]
mod fixtures;

#[cfg(test)]
#[path = "../../tests/unit/authorization/properties.rs"]
mod properties;
