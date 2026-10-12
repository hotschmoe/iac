// The float functions IEEE 754 does not pin to one answer, from one
// implementation on every target.
//
// The four operations, `sqrt`, rounding and casts are exact wherever Rust
// runs and are used as they come. Powers, logarithms and exponentials are
// not: std hands them to whatever the platform has, the system's libm on a
// native build and Rust's port of musl's in WebAssembly, and those part in
// the last bit. A world must play the same on a server and in a browser, so
// the engine takes them from here: the `libm` crate, which is that same port
// compiled for every target, and an integer power that multiplies in a fixed
// order. Clippy refuses the std methods in the engine crates (`clippy.toml`).

use std::ops::{Div, Mul};

pub trait Float: Copy + Mul<Output = Self> + Div<Output = Self> {
    const ONE: Self;
    fn powf(self, exponent: Self) -> Self;
    fn ln(self) -> Self;
    fn log2(self) -> Self;
    fn exp(self) -> Self;
    fn exp2(self) -> Self;
}

impl Float for f32 {
    const ONE: f32 = 1.0;

    fn powf(self, exponent: f32) -> f32 {
        libm::powf(self, exponent)
    }

    fn ln(self) -> f32 {
        libm::logf(self)
    }

    fn log2(self) -> f32 {
        libm::log2f(self)
    }

    fn exp(self) -> f32 {
        libm::expf(self)
    }

    fn exp2(self) -> f32 {
        libm::exp2f(self)
    }
}

impl Float for f64 {
    const ONE: f64 = 1.0;

    fn powf(self, exponent: f64) -> f64 {
        libm::pow(self, exponent)
    }

    fn ln(self) -> f64 {
        libm::log(self)
    }

    fn log2(self) -> f64 {
        libm::log2(self)
    }

    fn exp(self) -> f64 {
        libm::exp(self)
    }

    fn exp2(self) -> f64 {
        libm::exp2(self)
    }
}

pub fn powf<F: Float>(base: F, exponent: F) -> F {
    Float::powf(base, exponent)
}

pub fn ln<F: Float>(x: F) -> F {
    Float::ln(x)
}

pub fn log2<F: Float>(x: F) -> F {
    Float::log2(x)
}

pub fn exp<F: Float>(x: F) -> F {
    Float::exp(x)
}

pub fn exp2<F: Float>(x: F) -> F {
    Float::exp2(x)
}

/// `base` to a whole power by squaring, the low bit of the exponent first.
pub fn powi<F: Float>(base: F, exponent: i32) -> F {
    let mut square = base;
    let mut left = exponent.unsigned_abs();
    let mut product = F::ONE;
    loop {
        if left & 1 == 1 {
            product = product * square;
        }
        left >>= 1;
        if left == 0 {
            break;
        }
        square = square * square;
    }
    if exponent < 0 { F::ONE / product } else { product }
}
