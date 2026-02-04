//! Tensor abstraction for GPU compute.
//!
//! Provides a simple tensor type that can be used with `CubeCL` kernels.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::error::{ComputeError, Result};

/// Data type for tensor elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DType {
    /// 32-bit floating point.
    F32,
    /// 16-bit floating point.
    F16,
    /// Brain floating point (16-bit).
    Bf16,
    /// 64-bit floating point.
    F64,
    /// 32-bit integer.
    I32,
    /// 64-bit integer.
    I64,
}

impl DType {
    /// Get the size in bytes of this data type.
    #[must_use]
    pub const fn size_bytes(&self) -> usize {
        match self {
            Self::F32 | Self::I32 => 4,
            Self::F16 | Self::Bf16 => 2,
            Self::F64 | Self::I64 => 8,
        }
    }

    /// Get the name of this data type.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::F16 => "f16",
            Self::Bf16 => "bf16",
            Self::F64 => "f64",
            Self::I32 => "i32",
            Self::I64 => "i64",
        }
    }
}

impl fmt::Display for DType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Shape of a tensor.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Shape {
    dims: Vec<usize>,
}

impl Shape {
    /// Create a new shape from dimensions.
    #[must_use]
    pub fn new(dims: impl Into<Vec<usize>>) -> Self {
        Self { dims: dims.into() }
    }

    /// Create a scalar shape (0 dimensions).
    #[must_use]
    pub fn scalar() -> Self {
        Self { dims: vec![] }
    }

    /// Create a 1D shape.
    #[must_use]
    pub fn d1(size: usize) -> Self {
        Self { dims: vec![size] }
    }

    /// Create a 2D shape (matrix).
    #[must_use]
    pub fn d2(rows: usize, cols: usize) -> Self {
        Self {
            dims: vec![rows, cols],
        }
    }

    /// Create a 3D shape.
    #[must_use]
    pub fn d3(d0: usize, d1: usize, d2: usize) -> Self {
        Self {
            dims: vec![d0, d1, d2],
        }
    }

    /// Create a 4D shape (batch, channels, height, width).
    #[must_use]
    pub fn d4(batch: usize, channels: usize, height: usize, width: usize) -> Self {
        Self {
            dims: vec![batch, channels, height, width],
        }
    }

    /// Get the number of dimensions.
    #[must_use]
    pub fn ndim(&self) -> usize {
        self.dims.len()
    }

    /// Get the dimensions.
    #[must_use]
    pub fn dims(&self) -> &[usize] {
        &self.dims
    }

    /// Get total number of elements.
    #[must_use]
    pub fn numel(&self) -> usize {
        if self.dims.is_empty() {
            1
        } else {
            self.dims.iter().product()
        }
    }

    /// Get size of dimension.
    #[must_use]
    pub fn dim(&self, i: usize) -> Option<usize> {
        self.dims.get(i).copied()
    }

    /// Check if shapes are broadcastable.
    #[must_use]
    pub fn is_broadcastable_with(&self, other: &Self) -> bool {
        let max_dims = self.ndim().max(other.ndim());

        for i in 0..max_dims {
            let d1 = self
                .dims
                .get(self.ndim().saturating_sub(i + 1))
                .copied()
                .unwrap_or(1);
            let d2 = other
                .dims
                .get(other.ndim().saturating_sub(i + 1))
                .copied()
                .unwrap_or(1);

            if d1 != d2 && d1 != 1 && d2 != 1 {
                return false;
            }
        }
        true
    }
}

impl fmt::Display for Shape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
        for (i, d) in self.dims.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{d}")?;
        }
        write!(f, "]")
    }
}

impl<const N: usize> From<[usize; N]> for Shape {
    fn from(dims: [usize; N]) -> Self {
        Self::new(dims.to_vec())
    }
}

impl From<Vec<usize>> for Shape {
    fn from(dims: Vec<usize>) -> Self {
        Self::new(dims)
    }
}

/// Tensor metadata (shape and dtype, no data).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TensorMeta {
    /// Shape of the tensor.
    pub shape: Shape,
    /// Data type.
    pub dtype: DType,
}

impl TensorMeta {
    /// Create new tensor metadata.
    #[must_use]
    pub fn new(shape: impl Into<Shape>, dtype: DType) -> Self {
        Self {
            shape: shape.into(),
            dtype,
        }
    }

    /// Get total size in bytes.
    #[must_use]
    pub fn size_bytes(&self) -> usize {
        self.shape.numel() * self.dtype.size_bytes()
    }
}

/// CPU tensor for data transfer.
///
/// This is a simple tensor that holds data on the CPU.
/// Use this for loading data before transferring to GPU.
#[derive(Debug, Clone)]
pub struct CpuTensor {
    /// Tensor metadata.
    pub meta: TensorMeta,
    /// Raw data bytes.
    pub data: Vec<u8>,
}

impl CpuTensor {
    /// Create a new CPU tensor from f32 data.
    #[must_use]
    pub fn from_f32(shape: impl Into<Shape>, data: &[f32]) -> Self {
        let shape = shape.into();
        assert_eq!(
            shape.numel(),
            data.len(),
            "data length must match shape"
        );

        let bytes: Vec<u8> = data.iter().flat_map(|f| f.to_le_bytes()).collect();

        Self {
            meta: TensorMeta::new(shape, DType::F32),
            data: bytes,
        }
    }

    /// Create a tensor filled with zeros.
    #[must_use]
    pub fn zeros(shape: impl Into<Shape>, dtype: DType) -> Self {
        let shape = shape.into();
        let size = shape.numel() * dtype.size_bytes();
        Self {
            meta: TensorMeta::new(shape, dtype),
            data: vec![0u8; size],
        }
    }

    /// Create a tensor filled with ones.
    #[must_use]
    pub fn ones(shape: impl Into<Shape>) -> Self {
        let shape = shape.into();
        let numel = shape.numel();
        let data: Vec<f32> = vec![1.0; numel];
        Self::from_f32(shape, &data)
    }

    /// Get data as f32 slice.
    ///
    /// # Errors
    ///
    /// Returns error if dtype is not F32.
    pub fn as_f32(&self) -> Result<Vec<f32>> {
        if self.meta.dtype != DType::F32 {
            return Err(ComputeError::InvalidShape {
                expected: vec![],
                actual: vec![],
            });
        }

        let floats: Vec<f32> = self
            .data
            .chunks_exact(4)
            .map(|chunk| {
                // SAFETY: chunks_exact(4) guarantees each chunk is exactly 4 bytes
            let bytes: [u8; 4] = [chunk[0], chunk[1], chunk[2], chunk[3]];
                f32::from_le_bytes(bytes)
            })
            .collect();

        Ok(floats)
    }

    /// Get shape.
    #[must_use]
    pub fn shape(&self) -> &Shape {
        &self.meta.shape
    }

    /// Get dtype.
    #[must_use]
    pub fn dtype(&self) -> DType {
        self.meta.dtype
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dtype_size() {
        assert_eq!(DType::F32.size_bytes(), 4);
        assert_eq!(DType::F16.size_bytes(), 2);
        assert_eq!(DType::F64.size_bytes(), 8);
    }

    #[test]
    fn test_shape_numel() {
        assert_eq!(Shape::scalar().numel(), 1);
        assert_eq!(Shape::d1(10).numel(), 10);
        assert_eq!(Shape::d2(3, 4).numel(), 12);
        assert_eq!(Shape::d3(2, 3, 4).numel(), 24);
    }

    #[test]
    fn test_shape_display() {
        assert_eq!(Shape::scalar().to_string(), "[]");
        assert_eq!(Shape::d1(10).to_string(), "[10]");
        assert_eq!(Shape::d2(3, 4).to_string(), "[3, 4]");
    }

    #[test]
    fn test_shape_broadcastable() {
        // Same shapes are always broadcastable
        let a = Shape::d2(3, 4);
        let b = Shape::d2(3, 4);
        assert!(a.is_broadcastable_with(&b));

        // [3,4] and [3,5] - not broadcastable (different last dim)
        let d = Shape::d2(3, 5);
        assert!(!a.is_broadcastable_with(&d));

        // [3,4] and [1,4] - broadcastable (1 broadcasts to 3)
        let e = Shape::d2(1, 4);
        assert!(a.is_broadcastable_with(&e));

        // Scalar broadcasts with anything
        let scalar = Shape::scalar();
        assert!(a.is_broadcastable_with(&scalar));
    }

    #[test]
    fn test_cpu_tensor_from_f32() {
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let tensor = CpuTensor::from_f32([2, 2], &data);

        assert_eq!(tensor.shape(), &Shape::d2(2, 2));
        assert_eq!(tensor.dtype(), DType::F32);

        let retrieved = tensor.as_f32().unwrap();
        assert_eq!(retrieved, data);
    }

    #[test]
    fn test_cpu_tensor_zeros() {
        let tensor = CpuTensor::zeros([3, 4], DType::F32);
        assert_eq!(tensor.shape().numel(), 12);
        assert_eq!(tensor.data.len(), 48); // 12 * 4 bytes
    }

    #[test]
    fn test_cpu_tensor_ones() {
        let tensor = CpuTensor::ones([2, 2]);
        let data = tensor.as_f32().unwrap();
        assert!(data.iter().all(|&x| (x - 1.0).abs() < 1e-6));
    }

    #[test]
    fn test_tensor_meta_size() {
        let meta = TensorMeta::new([1024, 1024], DType::F32);
        assert_eq!(meta.size_bytes(), 1024 * 1024 * 4);
    }

    // =========================================================================
    // Additional Coverage Tests
    // =========================================================================

    #[test]
    fn test_dtype_names() {
        assert_eq!(DType::F32.name(), "f32");
        assert_eq!(DType::F16.name(), "f16");
        assert_eq!(DType::Bf16.name(), "bf16");
        assert_eq!(DType::F64.name(), "f64");
        assert_eq!(DType::I32.name(), "i32");
        assert_eq!(DType::I64.name(), "i64");
    }

    #[test]
    fn test_dtype_display() {
        assert_eq!(DType::F32.to_string(), "f32");
        assert_eq!(DType::F16.to_string(), "f16");
        assert_eq!(DType::Bf16.to_string(), "bf16");
    }

    #[test]
    fn test_dtype_size_all_types() {
        assert_eq!(DType::F32.size_bytes(), 4);
        assert_eq!(DType::F16.size_bytes(), 2);
        assert_eq!(DType::Bf16.size_bytes(), 2);
        assert_eq!(DType::F64.size_bytes(), 8);
        assert_eq!(DType::I32.size_bytes(), 4);
        assert_eq!(DType::I64.size_bytes(), 8);
    }

    #[test]
    fn test_dtype_equality() {
        assert_eq!(DType::F32, DType::F32);
        assert_ne!(DType::F32, DType::F64);
    }

    #[test]
    fn test_dtype_clone() {
        let dtype = DType::F32;
        let cloned = dtype;
        assert_eq!(dtype, cloned);
    }

    #[test]
    fn test_dtype_serialization() {
        let dtype = DType::F32;
        let json = serde_json::to_string(&dtype).expect("serialize");
        let deserialized: DType = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(dtype, deserialized);
    }

    #[test]
    fn test_shape_d4() {
        let shape = Shape::d4(2, 3, 224, 224);
        assert_eq!(shape.ndim(), 4);
        assert_eq!(shape.numel(), 2 * 3 * 224 * 224);
        assert_eq!(shape.dim(0), Some(2));
        assert_eq!(shape.dim(1), Some(3));
        assert_eq!(shape.dim(2), Some(224));
        assert_eq!(shape.dim(3), Some(224));
    }

    #[test]
    fn test_shape_dims() {
        let shape = Shape::d3(2, 3, 4);
        assert_eq!(shape.dims(), &[2, 3, 4]);
    }

    #[test]
    fn test_shape_dim_out_of_bounds() {
        let shape = Shape::d2(3, 4);
        assert_eq!(shape.dim(0), Some(3));
        assert_eq!(shape.dim(1), Some(4));
        assert_eq!(shape.dim(2), None);
        assert_eq!(shape.dim(100), None);
    }

    #[test]
    fn test_shape_equality() {
        let a = Shape::d2(3, 4);
        let b = Shape::d2(3, 4);
        let c = Shape::d2(4, 3);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_shape_clone() {
        let shape = Shape::d3(2, 3, 4);
        let cloned = shape.clone();
        assert_eq!(shape, cloned);
    }

    #[test]
    fn test_shape_from_array() {
        let shape: Shape = [2, 3, 4].into();
        assert_eq!(shape, Shape::d3(2, 3, 4));
    }

    #[test]
    fn test_shape_from_vec() {
        let shape: Shape = vec![2, 3, 4, 5].into();
        assert_eq!(shape.ndim(), 4);
        assert_eq!(shape.numel(), 120);
    }

    #[test]
    fn test_shape_serialization() {
        let shape = Shape::d2(3, 4);
        let json = serde_json::to_string(&shape).expect("serialize");
        let deserialized: Shape = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(shape, deserialized);
    }

    #[test]
    fn test_shape_broadcastable_different_ndim() {
        // Scalar broadcasts with anything (already tested but confirm)
        let a = Shape::d2(3, 4);
        let scalar = Shape::scalar();
        assert!(a.is_broadcastable_with(&scalar));

        // Same shape broadcasts
        let same = Shape::d2(3, 4);
        assert!(a.is_broadcastable_with(&same));

        // [3,4] and [1,4] - broadcastable (1 broadcasts to 3)
        let b = Shape::d2(1, 4);
        assert!(a.is_broadcastable_with(&b));

        // [3,4] and [3,1] - broadcastable (1 broadcasts to 4)
        let c = Shape::d2(3, 1);
        assert!(a.is_broadcastable_with(&c));
    }

    #[test]
    fn test_shape_broadcastable_with_ones() {
        // [5,3,4] and [1,1,4] - broadcastable
        let a = Shape::d3(5, 3, 4);
        let b = Shape::d3(1, 1, 4);
        assert!(a.is_broadcastable_with(&b));
    }

    #[test]
    fn test_tensor_meta_new() {
        let meta = TensorMeta::new([2, 3, 4], DType::F16);
        assert_eq!(meta.shape.numel(), 24);
        assert_eq!(meta.dtype, DType::F16);
    }

    #[test]
    fn test_tensor_meta_size_various_dtypes() {
        let f32_meta = TensorMeta::new([100], DType::F32);
        assert_eq!(f32_meta.size_bytes(), 400);

        let f16_meta = TensorMeta::new([100], DType::F16);
        assert_eq!(f16_meta.size_bytes(), 200);

        let f64_meta = TensorMeta::new([100], DType::F64);
        assert_eq!(f64_meta.size_bytes(), 800);
    }

    #[test]
    fn test_tensor_meta_equality() {
        let a = TensorMeta::new([3, 4], DType::F32);
        let b = TensorMeta::new([3, 4], DType::F32);
        let c = TensorMeta::new([4, 3], DType::F32);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_tensor_meta_serialization() {
        let meta = TensorMeta::new([2, 3], DType::F32);
        let json = serde_json::to_string(&meta).expect("serialize");
        let deserialized: TensorMeta = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(meta, deserialized);
    }

    #[test]
    fn test_cpu_tensor_from_f32_1d() {
        let data = vec![1.0, 2.0, 3.0];
        let tensor = CpuTensor::from_f32([3], &data);
        assert_eq!(tensor.shape(), &Shape::d1(3));
        assert_eq!(tensor.shape().numel(), 3);
    }

    #[test]
    fn test_cpu_tensor_from_f32_4d() {
        let data: Vec<f32> = (0..24).map(|x| x as f32).collect();
        let tensor = CpuTensor::from_f32([2, 3, 2, 2], &data);
        assert_eq!(tensor.shape().ndim(), 4);
        assert_eq!(tensor.shape().numel(), 24);
    }

    #[test]
    fn test_cpu_tensor_zeros_various_dtypes() {
        let f32_tensor = CpuTensor::zeros([10], DType::F32);
        assert_eq!(f32_tensor.meta.size_bytes(), 40);

        let f16_tensor = CpuTensor::zeros([10], DType::F16);
        assert_eq!(f16_tensor.meta.size_bytes(), 20);

        let i32_tensor = CpuTensor::zeros([10], DType::I32);
        assert_eq!(i32_tensor.meta.size_bytes(), 40);
    }

    #[test]
    fn test_cpu_tensor_ones_values() {
        let tensor = CpuTensor::ones([5]);
        let data = tensor.as_f32().unwrap();
        assert_eq!(data.len(), 5);
        for val in data {
            assert!((val - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_cpu_tensor_as_f32_wrong_dtype() {
        let tensor = CpuTensor::zeros([10], DType::F64);
        let result = tensor.as_f32();
        assert!(result.is_err());
    }

    #[test]
    fn test_cpu_tensor_data_access() {
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let tensor = CpuTensor::from_f32([4], &data);
        assert_eq!(tensor.data.len(), 16); // 4 elements * 4 bytes
    }

    #[test]
    fn test_cpu_tensor_clone() {
        let data = vec![1.0, 2.0, 3.0];
        let tensor = CpuTensor::from_f32([3], &data);
        let cloned = tensor.clone();
        assert_eq!(tensor.shape(), cloned.shape());
        assert_eq!(tensor.as_f32().unwrap(), cloned.as_f32().unwrap());
    }

    #[test]
    fn test_cpu_tensor_debug() {
        let tensor = CpuTensor::ones([2, 2]);
        let debug = format!("{:?}", tensor);
        assert!(debug.contains("CpuTensor"));
    }

    #[test]
    fn test_shape_large_tensor() {
        // Test with large dimensions
        let shape = Shape::d4(16, 512, 512, 512);
        assert_eq!(shape.numel(), 16 * 512 * 512 * 512);
    }

    #[test]
    fn test_shape_empty_like() {
        // Shape with zero dimension
        let shape = Shape::d2(0, 4);
        assert_eq!(shape.numel(), 0);
    }
}
