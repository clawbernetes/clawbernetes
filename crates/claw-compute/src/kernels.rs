//! Compute kernel stubs for multi-platform GPU operations.
//!
//! These kernels will be implemented using `CubeCL` once the runtime
//! integration is complete. For now, they serve as API documentation.

use crate::tensor::CpuTensor;
use crate::error::{ComputeError, Result};

/// Apply GELU activation function element-wise.
///
/// GELU(x) = x * 0.5 * (1 + erf(x / sqrt(2)))
///
/// This is the standard activation used in transformers.
pub fn gelu(input: &CpuTensor) -> Result<CpuTensor> {
    let data = input.as_f32()?;
    let sqrt2: f32 = 2.0_f32.sqrt();
    
    let output: Vec<f32> = data
        .iter()
        .map(|&x| x * 0.5 * (1.0 + libm::erff(x / sqrt2)))
        .collect();
    
    Ok(CpuTensor::from_f32(input.shape().clone(), &output))
}

/// Apply `ReLU` activation function element-wise.
///
/// ReLU(x) = max(0, x)
pub fn relu(input: &CpuTensor) -> Result<CpuTensor> {
    let data = input.as_f32()?;
    
    let output: Vec<f32> = data
        .iter()
        .map(|&x| x.max(0.0))
        .collect();
    
    Ok(CpuTensor::from_f32(input.shape().clone(), &output))
}

/// Apply `SiLU` (Swish) activation function element-wise.
///
/// SiLU(x) = x * sigmoid(x) = x / (1 + exp(-x))
pub fn silu(input: &CpuTensor) -> Result<CpuTensor> {
    let data = input.as_f32()?;
    
    let output: Vec<f32> = data
        .iter()
        .map(|&x| x / (1.0 + (-x).exp()))
        .collect();
    
    Ok(CpuTensor::from_f32(input.shape().clone(), &output))
}

/// Apply softmax along the last dimension.
pub fn softmax(input: &CpuTensor) -> Result<CpuTensor> {
    let data = input.as_f32()?;
    
    // Find max for numerical stability
    let max = data.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    
    // Compute exp(x - max)
    let exp_data: Vec<f32> = data.iter().map(|&x| (x - max).exp()).collect();
    
    // Sum for normalization
    let sum: f32 = exp_data.iter().sum();
    
    // Normalize
    let output: Vec<f32> = exp_data.iter().map(|&x| x / sum).collect();
    
    Ok(CpuTensor::from_f32(input.shape().clone(), &output))
}

/// Element-wise addition.
pub fn add(a: &CpuTensor, b: &CpuTensor) -> Result<CpuTensor> {
    if a.shape() != b.shape() {
        return Err(ComputeError::InvalidShape {
            expected: a.shape().dims().to_vec(),
            actual: b.shape().dims().to_vec(),
        });
    }
    
    let a_data = a.as_f32()?;
    let b_data = b.as_f32()?;
    
    let output: Vec<f32> = a_data
        .iter()
        .zip(b_data.iter())
        .map(|(&x, &y)| x + y)
        .collect();
    
    Ok(CpuTensor::from_f32(a.shape().clone(), &output))
}

/// Element-wise multiplication.
pub fn mul(a: &CpuTensor, b: &CpuTensor) -> Result<CpuTensor> {
    if a.shape() != b.shape() {
        return Err(ComputeError::InvalidShape {
            expected: a.shape().dims().to_vec(),
            actual: b.shape().dims().to_vec(),
        });
    }
    
    let a_data = a.as_f32()?;
    let b_data = b.as_f32()?;
    
    let output: Vec<f32> = a_data
        .iter()
        .zip(b_data.iter())
        .map(|(&x, &y)| x * y)
        .collect();
    
    Ok(CpuTensor::from_f32(a.shape().clone(), &output))
}

/// Scalar multiplication.
pub fn scale(input: &CpuTensor, scalar: f32) -> Result<CpuTensor> {
    let data = input.as_f32()?;
    
    let output: Vec<f32> = data.iter().map(|&x| x * scalar).collect();
    
    Ok(CpuTensor::from_f32(input.shape().clone(), &output))
}

/// Matrix multiplication (2D tensors only for now).
#[allow(clippy::many_single_char_names)]
pub fn matmul(lhs: &CpuTensor, rhs: &CpuTensor) -> Result<CpuTensor> {
    let lhs_shape = lhs.shape();
    let rhs_shape = rhs.shape();
    
    if lhs_shape.ndim() != 2 || rhs_shape.ndim() != 2 {
        return Err(ComputeError::UnsupportedOperation {
            platform: "CPU".to_string(),
            operation: "matmul requires 2D tensors".to_string(),
        });
    }
    
    // Safe to use indexing since we checked ndim == 2
    let rows = lhs_shape.dims()[0];
    let inner = lhs_shape.dims()[1];
    let inner2 = rhs_shape.dims()[0];
    let cols = rhs_shape.dims()[1];
    
    if inner != inner2 {
        return Err(ComputeError::InvalidShape {
            expected: vec![inner, cols],
            actual: vec![inner2, cols],
        });
    }
    
    let lhs_data = lhs.as_f32()?;
    let rhs_data = rhs.as_f32()?;
    
    let mut output = vec![0.0f32; rows * cols];
    
    // Simple O(n^3) matmul - production would use BLAS/CubeCL
    for row in 0..rows {
        for col in 0..cols {
            let mut sum = 0.0;
            for idx in 0..inner {
                sum += lhs_data[row * inner + idx] * rhs_data[idx * cols + col];
            }
            output[row * cols + col] = sum;
        }
    }
    
    Ok(CpuTensor::from_f32([rows, cols], &output))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gelu() {
        let input = CpuTensor::from_f32([4], &[-1.0, 0.0, 1.0, 2.0]);
        let output = gelu(&input).unwrap();
        let data = output.as_f32().unwrap();
        
        // GELU(0) ≈ 0
        assert!((data[1]).abs() < 0.01);
        // GELU(1) ≈ 0.841
        assert!((data[2] - 0.841).abs() < 0.01);
    }

    #[test]
    fn test_relu() {
        let input = CpuTensor::from_f32([4], &[-1.0, 0.0, 1.0, 2.0]);
        let output = relu(&input).unwrap();
        let data = output.as_f32().unwrap();
        
        assert_eq!(data, vec![0.0, 0.0, 1.0, 2.0]);
    }

    #[test]
    fn test_silu() {
        let input = CpuTensor::from_f32([3], &[0.0, 1.0, -1.0]);
        let output = silu(&input).unwrap();
        let data = output.as_f32().unwrap();
        
        // SiLU(0) = 0
        assert!((data[0]).abs() < 0.01);
        // SiLU(1) ≈ 0.731
        assert!((data[1] - 0.731).abs() < 0.01);
    }

    #[test]
    fn test_softmax() {
        let input = CpuTensor::from_f32([3], &[1.0, 2.0, 3.0]);
        let output = softmax(&input).unwrap();
        let data = output.as_f32().unwrap();
        
        // Sum should be 1
        let sum: f32 = data.iter().sum();
        assert!((sum - 1.0).abs() < 0.001);
        
        // Larger inputs should have larger probabilities
        assert!(data[2] > data[1]);
        assert!(data[1] > data[0]);
    }

    #[test]
    fn test_add() {
        let a = CpuTensor::from_f32([3], &[1.0, 2.0, 3.0]);
        let b = CpuTensor::from_f32([3], &[4.0, 5.0, 6.0]);
        let output = add(&a, &b).unwrap();
        let data = output.as_f32().unwrap();
        
        assert_eq!(data, vec![5.0, 7.0, 9.0]);
    }

    #[test]
    fn test_mul() {
        let a = CpuTensor::from_f32([3], &[1.0, 2.0, 3.0]);
        let b = CpuTensor::from_f32([3], &[4.0, 5.0, 6.0]);
        let output = mul(&a, &b).unwrap();
        let data = output.as_f32().unwrap();
        
        assert_eq!(data, vec![4.0, 10.0, 18.0]);
    }

    #[test]
    fn test_scale() {
        let input = CpuTensor::from_f32([3], &[1.0, 2.0, 3.0]);
        let output = scale(&input, 2.0).unwrap();
        let data = output.as_f32().unwrap();
        
        assert_eq!(data, vec![2.0, 4.0, 6.0]);
    }

    #[test]
    fn test_matmul() {
        // [2,3] @ [3,2] = [2,2]
        let a = CpuTensor::from_f32([2, 3], &[
            1.0, 2.0, 3.0,
            4.0, 5.0, 6.0,
        ]);
        let b = CpuTensor::from_f32([3, 2], &[
            7.0, 8.0,
            9.0, 10.0,
            11.0, 12.0,
        ]);
        let output = matmul(&a, &b).unwrap();
        let data = output.as_f32().unwrap();
        
        // Expected: [[58, 64], [139, 154]]
        assert_eq!(data, vec![58.0, 64.0, 139.0, 154.0]);
    }

    #[test]
    fn test_matmul_shape_mismatch() {
        let a = CpuTensor::from_f32([2, 3], &[1.0; 6]);
        let b = CpuTensor::from_f32([2, 2], &[1.0; 4]);
        
        assert!(matmul(&a, &b).is_err());
    }
}
