//! Provides BLAS for a CUDA backend.
#![allow(missing_docs)]
use ::plugin::*;
use collenchyma::backend::Backend;
use collenchyma::tensor::ITensorDesc;
use collenchyma::plugin::Error as PluginError;
use collenchyma::frameworks::cuda::Cuda;
use cublas;

#[macro_use]
pub mod helper;

lazy_static! {
    static ref CONTEXT: cublas::Context = {
        let mut context = cublas::Context::new().unwrap();
        context.set_pointer_mode(cublas::api::PointerMode::Device).unwrap();
        context
    };
}

impl Asum<f32> for Backend<Cuda> {
    iblas_asum_for_cuda!(f32);
}

impl Axpy<f32> for Backend<Cuda> {
    iblas_axpy_for_cuda!(f32);
}

impl Copy<f32> for Backend<Cuda> {
    iblas_copy_for_cuda!(f32);
}

impl Dot<f32> for Backend<Cuda> {
    iblas_dot_for_cuda!(f32);
}

impl Nrm2<f32> for Backend<Cuda> {
    iblas_nrm2_for_cuda!(f32);
}

impl Scal<f32> for Backend<Cuda> {
    iblas_scal_for_cuda!(f32);
}

impl Swap<f32> for Backend<Cuda> {
    iblas_swap_for_cuda!(f32);
}

impl Gemm<f32> for Backend<Cuda> {
    iblas_gemm_for_cuda!(f32);
}
