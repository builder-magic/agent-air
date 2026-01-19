# deepmesa-ai

High-performance matrix operations and AI algorithms in Rust.

## Overview

This crate provides optimized matrix operations and linear algebra functionality for AI and machine learning applications. It includes:

- **Matrix Operations**: High-performance row-major matrix implementation with SIMD optimizations
- **Linear Algebra**: Matrix multiplication (GEMM), addition, subtraction, and scalar operations
- **Regression Models**: Linear regression with ordinary least squares
- **Memory Management**: Custom allocators for optimal performance

## Features

- Zero-dependency core (only uses `std`, `alloc`, `core`)
- Hand-optimized matrix operations
- Comprehensive test suite
- Memory-efficient implementations
- Extensive API for matrix manipulation

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
deepmesa-ai = "0.10.0"
```

### Basic Matrix Operations

```rust
use deepmesa_ai::matrix::Matrix;

// Create a new 3x3 matrix
let mut matrix = Matrix::<f64>::new(3, 3);

// Fill with values
matrix.set(0, 0, 1.0);
matrix.set(0, 1, 2.0);
matrix.set(0, 2, 3.0);

// Get values
let value = matrix.get(0, 0); // Returns 1.0

// Matrix arithmetic
let matrix2 = Matrix::<f64>::new(3, 3);
let result = matrix + matrix2; // Matrix addition
```

### Matrix Multiplication

```rust
use deepmesa_ai::matrix::Matrix;

let a = Matrix::<f64>::new(2, 3);
let b = Matrix::<f64>::new(3, 2);
let result = a * b; // 2x2 result matrix
```

## Performance

This library is designed for high-performance computing with:

- Cache-friendly memory layouts
- Vectorized operations where possible
- Minimal allocations
- Optimized algorithms

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

## Contributing

This crate is part of the DeepMesa project. For contributions and issues, please visit the main repository.
