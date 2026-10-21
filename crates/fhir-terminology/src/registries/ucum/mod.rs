// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! UCUM, the Unified Code for Units of Measure (`http://unitsofmeasure.org`).
//!
//! A grammar over the unit definitions of `ucum-essence.xml`
//! (<https://ucum.org/ucum>, <https://terminology.hl7.org/UCUM.html>).
//!
//! A code is any expression the grammar accepts whose atoms the essence
//! defines; the expression is its own display. Two expressions are the same
//! unit when their canonical forms agree in dimension and magnitude. The
//! grammar "generates an infinite number of codes", so the system is never
//! enumerated.

pub mod canonical;
pub mod essence;
pub mod grammar;
pub mod provider;
