//! Assemblies — the formula engine (addendum: derived quantities).
//!
//! An [`Assembly`] is a reusable recipe: parameters plus formula-driven
//! [`Part`]s. [`apply`]ing it to a measurement yields a [`BillOfMaterials`]
//! whose every line carries the formula that produced it — provenance for
//! derived quantities, not just measured ones. BOMs are DERIVED (invariant
//! 5): recompute from measurement + assembly on read; never persist them
//! as truth (they carry no serde derive, structurally).

pub mod expr;

pub use expr::ExprError;
