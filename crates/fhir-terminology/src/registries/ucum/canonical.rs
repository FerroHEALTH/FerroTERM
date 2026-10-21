// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Dimensional analysis: an expression reduced to a magnitude over the seven
//! base units (<https://ucum.org/ucum> §2.3 "Semantics").

use std::collections::BTreeMap;

use super::essence::Essence;
use super::grammar::{Atom, Expression, GrammarError};

/// The number of base units (dimensions).
pub const DIMENSIONS: usize = 7;

/// A canonical form: `magnitude` times the base units raised to `dimensions`.
#[derive(Debug, Clone, PartialEq)]
pub struct Canonical {
    /// The factor over the base units.
    pub magnitude: f64,
    /// The exponent of each base unit, in essence order.
    pub dimensions: [i32; DIMENSIONS],
    /// The special-unit function this form passes through, when any
    /// (`Cel`, `[degF]`, the logarithmic units): such a form is not a plain
    /// scale of its base.
    pub special: Option<String>,
    /// Whether an arbitrary unit is involved: not commensurable with anything.
    pub arbitrary: bool,
}

impl Canonical {
    /// The dimensionless unit `1`.
    #[must_use]
    pub fn one() -> Self {
        Self {
            magnitude: 1.0,
            dimensions: [0; DIMENSIONS],
            special: None,
            arbitrary: false,
        }
    }

    fn scale(&mut self, factor: f64, exponent: i32) {
        self.magnitude *= factor.powi(exponent);
    }

    fn multiply(&mut self, other: &Self, exponent: i32) {
        self.magnitude *= other.magnitude.powi(exponent);
        for (d, o) in self.dimensions.iter_mut().zip(other.dimensions) {
            *d += o * exponent;
        }
        if other.special.is_some() {
            self.special.clone_from(&other.special);
        }
        self.arbitrary |= other.arbitrary;
    }

    /// Whether two forms are the same unit: equal dimensions, equal
    /// magnitude (to one part in a billion), the same special function, and
    /// neither arbitrary.
    #[must_use]
    pub fn same_unit(&self, other: &Self) -> bool {
        self.dimensions == other.dimensions
            && self.special == other.special
            && !self.arbitrary
            && !other.arbitrary
            && (self.magnitude - other.magnitude).abs()
                <= self.magnitude.abs().max(other.magnitude.abs()) * 1e-9
    }

    /// Whether two forms measure the same kind of quantity.
    #[must_use]
    pub fn commensurable(&self, other: &Self) -> bool {
        self.dimensions == other.dimensions && !self.arbitrary && !other.arbitrary
    }

    /// The base-unit form as text (`g.m-1`, `1` when dimensionless), the
    /// magnitude dropped, in essence order of the base units.
    #[must_use]
    pub fn text(&self, essence: &Essence) -> String {
        let mut bases: Vec<(&str, i32)> = essence
            .base_units
            .values()
            .filter_map(|b| {
                self.dimensions
                    .get(b.dimension)
                    .filter(|e| **e != 0)
                    .map(|e| (b.code.as_str(), *e))
            })
            .collect();
        bases.sort_by_key(|(code, _)| essence.base_units.get(*code).map_or(0, |b| b.dimension));
        if bases.is_empty() {
            return String::from("1");
        }
        bases
            .iter()
            .map(|(code, e)| {
                if *e == 1 {
                    (*code).to_owned()
                } else {
                    format!("{code}{e}")
                }
            })
            .collect::<Vec<_>>()
            .join(".")
    }

    /// The property of the quantity (`mass`, `length`, …) when the form is a
    /// single base unit, else `None`.
    #[must_use]
    pub fn base_property<'a>(&self, essence: &'a Essence) -> Option<&'a str> {
        let mut single = None;
        for (index, exponent) in self.dimensions.iter().enumerate() {
            match (*exponent, single) {
                (0, _) => {}
                (1, None) => single = Some(index),
                _ => return None,
            }
        }
        let index = single?;
        essence
            .base_units
            .values()
            .find(|b| b.dimension == index)
            .map(|b| b.property.as_str())
    }
}

/// Reduces expressions to canonical forms over an essence, memoizing the
/// units it has already reduced.
#[derive(Debug)]
pub struct Reducer<'a> {
    essence: &'a Essence,
    units: BTreeMap<String, Canonical>,
}

impl<'a> Reducer<'a> {
    /// A reducer over `essence`.
    #[must_use]
    pub fn new(essence: &'a Essence) -> Self {
        Self {
            essence,
            units: BTreeMap::new(),
        }
    }

    /// The canonical form of `expression`.
    ///
    /// # Errors
    ///
    /// Returns [`GrammarError`] when a unit's own definition does not parse
    /// (a defect of the essence).
    pub fn reduce(&mut self, expression: &Expression) -> Result<Canonical, GrammarError> {
        let mut out = Canonical::one();
        for component in &expression.components {
            match &component.atom {
                Atom::Factor(n) => {
                    #[expect(
                        clippy::as_conversions,
                        clippy::cast_precision_loss,
                        reason = "a unit factor is small"
                    )]
                    out.scale(*n as f64, component.exponent);
                }
                Atom::Annotation => {}
                Atom::Group(inner) => {
                    let reduced = self.reduce(inner)?;
                    out.multiply(&reduced, component.exponent);
                }
                Atom::Unit { prefix, code } => {
                    if let Some(prefix) = prefix
                        && let Some(p) = self.essence.prefixes.get(prefix)
                    {
                        out.scale(p.value, component.exponent);
                    }
                    let unit = self.unit(code)?;
                    out.multiply(&unit, component.exponent);
                }
            }
        }
        Ok(out)
    }

    /// The canonical form of one atom of the essence.
    fn unit(&mut self, code: &str) -> Result<Canonical, GrammarError> {
        if let Some(known) = self.units.get(code) {
            return Ok(known.clone());
        }
        let canonical = if let Some(base) = self.essence.base_units.get(code) {
            let mut form = Canonical::one();
            if let Some(d) = form.dimensions.get_mut(base.dimension) {
                *d = 1;
            }
            form
        } else if let Some(unit) = self.essence.units.get(code) {
            let (definition, factor, special, arbitrary) = (
                unit.definition.clone(),
                unit.factor,
                unit.is_special.then(|| unit.code.clone()),
                unit.is_arbitrary,
            );
            let mut form = if definition.is_empty() || definition == "1" {
                Canonical::one()
            } else {
                let parsed = super::grammar::parse(&definition, self.essence)?;
                self.reduce(&parsed)?
            };
            form.magnitude *= factor;
            if special.is_some() {
                form.special = special;
            }
            form.arbitrary |= arbitrary;
            form
        } else {
            return Err(GrammarError::UnknownAtom {
                symbol: code.to_owned(),
            });
        };
        self.units.insert(code.to_owned(), canonical.clone());
        Ok(canonical)
    }
}
