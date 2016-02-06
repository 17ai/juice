//! Provides NN for a CUDA backend.
#![allow(missing_docs)]
// use co::backend::Backend;
// use co::device::DeviceType;
// use co::tensor::{SharedTensor, ITensorDesc};
// use co::frameworks::cuda::Cuda;
use co::prelude::*;
use co::Error as CoError;
use co::plugin::Error as PluginError;
use cudnn::*;
use ::plugin::*;

#[macro_use]
pub mod helper;

lazy_static! {
    static ref CUDNN: Cudnn = Cudnn::new().unwrap();
}

pub trait ICudnnDesc<T> {
    fn cudnn_tensor_desc(&self) -> Result<TensorDescriptor, PluginError>;

    fn cudnn_filter_desc(&self) -> Result<FilterDescriptor, PluginError>;

    fn cudnn_convolution_desc(&self, filter: &SharedTensor<T>) -> Result<ConvolutionDescriptor, PluginError>;
}

impl ICudnnDesc<f32> for SharedTensor<f32> {
    fn cudnn_tensor_desc(&self) -> Result<TensorDescriptor, PluginError> {
        match TensorDescriptor::new(&self.desc().dims_i32().clone(), &self.desc().default_stride_i32().clone(), utils::DataType::Float) {
            Ok(desc) => Ok(desc),
            Err(_) => {
                Err(PluginError::Plugin("Unable to create CuDNN TensorDescriptor."))
            }
        }
    }

    fn cudnn_filter_desc(&self) -> Result<FilterDescriptor, PluginError> {
        match FilterDescriptor::new(&self.desc().dims_i32().clone(), utils::DataType::Float) {
            Ok(desc) => Ok(desc),
            Err(_) => {
                Err(PluginError::Plugin("Unable to create CuDNN FilterDescriptor."))
            }
        }
    }

    fn cudnn_convolution_desc(&self, filter: &SharedTensor<f32>) -> Result<ConvolutionDescriptor, PluginError> {
        match ConvolutionDescriptor::new(&self.desc().dims_i32().clone(), &filter.desc().default_stride_i32().clone(), utils::DataType::Float) {
            Ok(desc) => Ok(desc),
            Err(_) => {
                Err(PluginError::Plugin("Unable to create CuDNN ConvolutionDescriptor."))
            }
        }
    }
}

impl ICudnnDesc<f64> for SharedTensor<f64> {
    fn cudnn_tensor_desc(&self) -> Result<TensorDescriptor, PluginError> {
        match TensorDescriptor::new(&self.desc().dims_i32().clone(), &self.desc().default_stride_i32().clone(), utils::DataType::Double) {
            Ok(desc) => Ok(desc),
            Err(_) => {
                Err(PluginError::Plugin("Unable to create CuDNN TensorDescriptor."))
            }
        }
    }

    fn cudnn_filter_desc(&self) -> Result<FilterDescriptor, PluginError> {
        match FilterDescriptor::new(&self.desc().dims_i32().clone(), utils::DataType::Double) {
            Ok(desc) => Ok(desc),
            Err(_) => {
                Err(PluginError::Plugin("Unable to create CuDNN FilterDescriptor."))
            }
        }
    }

    fn cudnn_convolution_desc(&self, filter: &SharedTensor<f64>) -> Result<ConvolutionDescriptor, PluginError> {
        match ConvolutionDescriptor::new(&self.desc().dims_i32().clone(), &filter.desc().default_stride_i32().clone(), utils::DataType::Double) {
            Ok(desc) => Ok(desc),
            Err(_) => {
                Err(PluginError::Plugin("Unable to create CuDNN ConvolutionDescriptor."))
            }
        }
    }
}

impl_oconf_for_cc!(f32, f64);
impl_oconf_for_clrn!(f32, f64);
impl_oconf_for_pooling!(f32, f64);

impl ConvForwardAlgo {
    /// Tries to return the matching cuDNN type for the enum value.
    fn as_cudnn(&self) -> Result<cudnnConvolutionFwdAlgo_t, CoError> {
        Ok(match *self {
            ConvForwardAlgo::Auto => return Err(CoError::Plugin(PluginError::Plugin("Can't create cuDNN convolution forward algorithm from ConvForwardAlgo::Auto. Use `find_cudnn_algo` to find an algorithm."))),
            ConvForwardAlgo::GEMM => ::cudnn::cudnnConvolutionFwdAlgo_t::CUDNN_CONVOLUTION_FWD_ALGO_GEMM,
            ConvForwardAlgo::ImplicitGEMM => ::cudnn::cudnnConvolutionFwdAlgo_t::CUDNN_CONVOLUTION_FWD_ALGO_IMPLICIT_GEMM,
            ConvForwardAlgo::ImplicitPrecompiledGEMM => ::cudnn::cudnnConvolutionFwdAlgo_t::CUDNN_CONVOLUTION_FWD_ALGO_IMPLICIT_PRECOMP_GEMM,
            ConvForwardAlgo::FFT => ::cudnn::cudnnConvolutionFwdAlgo_t::CUDNN_CONVOLUTION_FWD_ALGO_FFT,
            ConvForwardAlgo::Direct => ::cudnn::cudnnConvolutionFwdAlgo_t::CUDNN_CONVOLUTION_FWD_ALGO_DIRECT,
        })
    }

    /// Returns the matching enum value for a cuDNN algo.
    fn from_cudnn(algo: &cudnnConvolutionFwdAlgo_t) -> ConvForwardAlgo {
        match *algo {
            ::cudnn::cudnnConvolutionFwdAlgo_t::CUDNN_CONVOLUTION_FWD_ALGO_GEMM => ConvForwardAlgo::GEMM,
            ::cudnn::cudnnConvolutionFwdAlgo_t::CUDNN_CONVOLUTION_FWD_ALGO_IMPLICIT_GEMM => ConvForwardAlgo::ImplicitGEMM,
            ::cudnn::cudnnConvolutionFwdAlgo_t::CUDNN_CONVOLUTION_FWD_ALGO_IMPLICIT_PRECOMP_GEMM => ConvForwardAlgo::ImplicitPrecompiledGEMM,
            ::cudnn::cudnnConvolutionFwdAlgo_t::CUDNN_CONVOLUTION_FWD_ALGO_FFT => ConvForwardAlgo::FFT,
            ::cudnn::cudnnConvolutionFwdAlgo_t::CUDNN_CONVOLUTION_FWD_ALGO_DIRECT => ConvForwardAlgo::Direct,
        }
    }

    /// Try to find best algorithm for a operation that uses the provided descriptors.
    fn find_cudnn_algo(
        &self,
        filter_desc: &FilterDescriptor,
        conv_desc: &ConvolutionDescriptor,
        src_desc: &TensorDescriptor,
        dest_desc: &TensorDescriptor,
    ) -> Result<ConvForwardAlgo, CoError> {
        if !self.is_auto() {
            return Ok(*self);
        }
        let algos = API::find_convolution_forward_algorithm(*CUDNN.id_c(), *filter_desc.id_c(), *conv_desc.id_c(), *src_desc.id_c(), *dest_desc.id_c()).unwrap();
        let algo = match algos.len() {
            0 => return Err(CoError::Plugin(PluginError::Operation("Unable to find CUDA cuDNN convolution forward algorithm."))),
            _ => algos[0].algo
        };
        Ok(ConvForwardAlgo::from_cudnn(&algo))
    }

    /// Check if the algo needs a cudnn workspace.
    fn needs_cudnn_workspace(&self) -> Result<bool, CoError> {
        Ok(match *self {
            ConvForwardAlgo::Auto => return Err(CoError::Plugin(PluginError::Plugin("Can't check necessary workspace size for ConvForwardAlgo::Auto. Use `find_cudnn_algo` to find an algorithm."))),
            ConvForwardAlgo::GEMM => true,
            ConvForwardAlgo::ImplicitGEMM => false,
            ConvForwardAlgo::ImplicitPrecompiledGEMM => true,
            ConvForwardAlgo::FFT => true,
            ConvForwardAlgo::Direct => true,
        })
    }
}

impl ConvBackwardFilterAlgo {
    /// Tries to return the matching cuDNN type for the enum value.
    fn as_cudnn(&self) -> Result<cudnnConvolutionBwdFilterAlgo_t, CoError> {
        Ok(match *self {
            ConvBackwardFilterAlgo::Auto => return Err(CoError::Plugin(PluginError::Plugin("Can't create cuDNN convolution backward filter algorithm from ConvBackwardFilterAlgo::Auto. Use `find_cudnn_algo` to find an algorithm."))),
            ConvBackwardFilterAlgo::ImplicitGEMM => ::cudnn::cudnnConvolutionBwdFilterAlgo_t::CUDNN_CONVOLUTION_BWD_FILTER_ALGO_1,
            ConvBackwardFilterAlgo::ImplicitGEMMSum => ::cudnn::cudnnConvolutionBwdFilterAlgo_t::CUDNN_CONVOLUTION_BWD_FILTER_ALGO_0,
            ConvBackwardFilterAlgo::ImplicitPrecompiledGEMMSum => ::cudnn::cudnnConvolutionBwdFilterAlgo_t::CUDNN_CONVOLUTION_BWD_FILTER_ALGO_3,
            ConvBackwardFilterAlgo::FFT => ::cudnn::cudnnConvolutionBwdFilterAlgo_t::CUDNN_CONVOLUTION_BWD_FILTER_ALGO_FFT,
        })
    }

    /// Returns the matching enum value for a cuDNN algo.
    fn from_cudnn(algo: &cudnnConvolutionBwdFilterAlgo_t) -> ConvBackwardFilterAlgo {
        match *algo {
            ::cudnn::cudnnConvolutionBwdFilterAlgo_t::CUDNN_CONVOLUTION_BWD_FILTER_ALGO_0 => ConvBackwardFilterAlgo::ImplicitGEMMSum,
            ::cudnn::cudnnConvolutionBwdFilterAlgo_t::CUDNN_CONVOLUTION_BWD_FILTER_ALGO_1 => ConvBackwardFilterAlgo::ImplicitGEMM,
            ::cudnn::cudnnConvolutionBwdFilterAlgo_t::CUDNN_CONVOLUTION_BWD_FILTER_ALGO_FFT => ConvBackwardFilterAlgo::FFT,
            ::cudnn::cudnnConvolutionBwdFilterAlgo_t::CUDNN_CONVOLUTION_BWD_FILTER_ALGO_3 => ConvBackwardFilterAlgo::ImplicitPrecompiledGEMMSum,
        }
    }

    /// Try to find best algorithm for a operation that uses the provided descriptors.
    fn find_cudnn_algo(
        &self,
        filter_desc: &FilterDescriptor,
        conv_desc: &ConvolutionDescriptor,
        src_desc: &TensorDescriptor,
        dest_desc: &TensorDescriptor,
    ) -> Result<ConvBackwardFilterAlgo, CoError> {
        if !self.is_auto() {
            return Ok(*self);
        }
        let algos = API::find_convolution_backward_filter_algorithm(*CUDNN.id_c(), *filter_desc.id_c(), *conv_desc.id_c(), *src_desc.id_c(), *dest_desc.id_c()).unwrap();
        let algo = match algos.len() {
            0 => return Err(CoError::Plugin(PluginError::Operation("Unable to find CUDA cuDNN convolution backward filter algorithm."))),
            _ => algos[0].algo
        };
        Ok(ConvBackwardFilterAlgo::from_cudnn(&algo))
    }

    /// Check if the algo needs a cudnn workspace.
    fn needs_cudnn_workspace(&self) -> Result<bool, CoError> {
        Ok(match *self {
            ConvBackwardFilterAlgo::Auto => return Err(CoError::Plugin(PluginError::Plugin("Can't check necessary workspace size for ConvBackwardFilterAlgo::Auto. Use `find_cudnn_algo` to find an algorithm."))),
            ConvBackwardFilterAlgo::ImplicitGEMM => false,
            ConvBackwardFilterAlgo::ImplicitGEMMSum => false,
            ConvBackwardFilterAlgo::ImplicitPrecompiledGEMMSum => true,
            ConvBackwardFilterAlgo::FFT => true,
        })
    }
}

impl ConvBackwardDataAlgo {
    /// Tries to return the matching cuDNN type for the enum value.
    fn as_cudnn(&self) -> Result<cudnnConvolutionBwdDataAlgo_t, CoError> {
        Ok(match *self {
            ConvBackwardDataAlgo::Auto => return Err(CoError::Plugin(PluginError::Plugin("Can't create cuDNN convolution backward data algorithm from ConvBackwardDataAlgo::Auto. Use `find_cudnn_algo` to find an algorithm."))),
            ConvBackwardDataAlgo::ImplicitGEMM => ::cudnn::cudnnConvolutionBwdDataAlgo_t::CUDNN_CONVOLUTION_BWD_DATA_ALGO_1,
            ConvBackwardDataAlgo::ImplicitGEMMSum => ::cudnn::cudnnConvolutionBwdDataAlgo_t::CUDNN_CONVOLUTION_BWD_DATA_ALGO_0,
            ConvBackwardDataAlgo::FFT => ::cudnn::cudnnConvolutionBwdDataAlgo_t::CUDNN_CONVOLUTION_BWD_DATA_ALGO_FFT,
        })
    }

    /// Returns the matching enum value for a cuDNN algo.
    fn from_cudnn(algo: &cudnnConvolutionBwdDataAlgo_t) -> ConvBackwardDataAlgo {
        match *algo {
            ::cudnn::cudnnConvolutionBwdDataAlgo_t::CUDNN_CONVOLUTION_BWD_DATA_ALGO_0 => ConvBackwardDataAlgo::ImplicitGEMMSum,
            ::cudnn::cudnnConvolutionBwdDataAlgo_t::CUDNN_CONVOLUTION_BWD_DATA_ALGO_1 => ConvBackwardDataAlgo::ImplicitGEMM,
            ::cudnn::cudnnConvolutionBwdDataAlgo_t::CUDNN_CONVOLUTION_BWD_DATA_ALGO_FFT => ConvBackwardDataAlgo::FFT,
        }
    }

    /// Try to find best algorithm for a operation that uses the provided descriptors.
    fn find_cudnn_algo(
        &self,
        filter_desc: &FilterDescriptor,
        conv_desc: &ConvolutionDescriptor,
        src_desc: &TensorDescriptor,
        dest_desc: &TensorDescriptor,
    ) -> Result<ConvBackwardDataAlgo, CoError> {
        if !self.is_auto() {
            return Ok(*self);
        }
        let algos = API::find_convolution_backward_data_algorithm(*CUDNN.id_c(), *filter_desc.id_c(), *conv_desc.id_c(), *src_desc.id_c(), *dest_desc.id_c()).unwrap();
        let algo = match algos.len() {
            0 => return Err(CoError::Plugin(PluginError::Operation("Unable to find CUDA cuDNN convolution backward data algorithm."))),
            _ => algos[0].algo
        };
        Ok(ConvBackwardDataAlgo::from_cudnn(&algo))
    }

    /// Check if the algo needs a cudnn workspace.
    fn needs_cudnn_workspace(&self) -> Result<bool, CoError> {
        Ok(match *self {
            ConvBackwardDataAlgo::Auto => return Err(CoError::Plugin(PluginError::Plugin("Can't check necessary workspace size for ConvBackwardDataAlgo::Auto. Use `find_cudnn_algo` to find an algorithm."))),
            ConvBackwardDataAlgo::ImplicitGEMM => false,
            ConvBackwardDataAlgo::ImplicitGEMMSum => false,
            ConvBackwardDataAlgo::FFT => true,
        })
    }
}

macro_rules! impl_convolution_for_cuda_backend {
    ($t:ty, $cutype:path) => (
        impl Convolution<$t> for Backend<Cuda> {
            fn new_convolution_config(
                &self,
                src: &SharedTensor<$t>,
                dest: &SharedTensor<$t>,
                filter: &mut SharedTensor<$t>,
                algo_fwd: ConvForwardAlgo,
                algo_bwd_filter: ConvBackwardFilterAlgo,
                algo_bwd_data: ConvBackwardDataAlgo,
                stride: &[i32],
                zero_padding: &[i32],
            ) -> Result<Self::CC, CoError> {
                let src_desc = try!(src.cudnn_tensor_desc());
                let dest_desc = try!(dest.cudnn_tensor_desc());
                let filter_desc = try!(filter.cudnn_filter_desc());
                let conv_desc = ::cudnn::ConvolutionDescriptor::new(zero_padding, stride, $cutype).unwrap();

                let useable_algo_fwd = try!(algo_fwd.find_cudnn_algo(&filter_desc, &conv_desc, &src_desc, &dest_desc));
                let (workspace_fwd, workspace_size_fwd) = match try!(useable_algo_fwd.needs_cudnn_workspace()) {
                    false => (::co::frameworks::cuda::Memory::from_c(0), 0),
                    true => {
                        let workspace_size_fwd = API::get_convolution_forward_workspace_size(*CUDNN.id_c(), useable_algo_fwd.as_cudnn().unwrap(), *filter_desc.id_c(), *conv_desc.id_c(), *src_desc.id_c(), *dest_desc.id_c()).unwrap();
                        let workspace_forward = ::co::frameworks::cuda::Memory::new(workspace_size_fwd).unwrap();
                        (workspace_forward, workspace_size_fwd)
                    }
                };

                let useable_algo_bwd_filter = try!(algo_bwd_filter.find_cudnn_algo(&filter_desc, &conv_desc, &src_desc, &dest_desc));
                let (workspace_bwd_filter, workspace_size_bwd_filter) = match try!(useable_algo_bwd_filter.needs_cudnn_workspace()) {
                    false => (::co::frameworks::cuda::Memory::from_c(0), 0),
                    true => {
                            let workspace_size_bwd_filter = API::get_convolution_backward_filter_workspace_size(*CUDNN.id_c(), useable_algo_bwd_filter.as_cudnn().unwrap(), *filter_desc.id_c(), *conv_desc.id_c(), *src_desc.id_c(), *dest_desc.id_c()).unwrap();
                            let workspace_backward = ::co::frameworks::cuda::Memory::new(workspace_size_bwd_filter).unwrap();
                            (workspace_backward, workspace_size_bwd_filter)
                    }
                };

                let useable_algo_bwd_data = try!(algo_bwd_data.find_cudnn_algo(&filter_desc, &conv_desc, &src_desc, &dest_desc));
                let (workspace_bwd_data, workspace_size_bwd_data) = match try!(useable_algo_bwd_data.needs_cudnn_workspace()) {
                    false => (::co::frameworks::cuda::Memory::from_c(0), 0),
                    true => {
                            let workspace_size_bwd_data = API::get_convolution_backward_data_workspace_size(*CUDNN.id_c(), useable_algo_bwd_data.as_cudnn().unwrap(), *filter_desc.id_c(), *conv_desc.id_c(), *src_desc.id_c(), *dest_desc.id_c()).unwrap();
                            let workspace_backward = ::co::frameworks::cuda::Memory::new(workspace_size_bwd_data).unwrap();
                            (workspace_backward, workspace_size_bwd_data)
                    }
                };

                // share one workspace to reduce memory
                let workspace: ::co::frameworks::cuda::Memory;
                if workspace_size_bwd_data >= workspace_size_bwd_filter && workspace_size_bwd_data >= workspace_size_fwd {
                    workspace = workspace_bwd_data;
                } else if workspace_size_bwd_filter >= workspace_size_bwd_data && workspace_size_bwd_filter >= workspace_size_fwd {
                    workspace = workspace_bwd_filter;
                } else {
                    workspace = workspace_fwd;
                }

                let workspace_bwd_fiter = ::co::frameworks::cuda::Memory::from_c(*workspace.id_c());
                let workspace_fwd = ::co::frameworks::cuda::Memory::from_c(*workspace.id_c());

                Ok(
                    ::cudnn::utils::ConvolutionConfig::new(
                        useable_algo_fwd.as_cudnn().unwrap(), workspace_fwd, workspace_size_fwd,
                        useable_algo_bwd_filter.as_cudnn().unwrap(), workspace_bwd_fiter, workspace_size_bwd_filter,
                        useable_algo_bwd_data.as_cudnn().unwrap(), workspace, workspace_size_bwd_data,
                        conv_desc, filter_desc
                    )
                )
            }

            impl_ops_convolution_for!($t, Backend<Cuda>);
        }
    )
}

impl NN<f32> for Backend<Cuda> {
    type CC = utils::ConvolutionConfig;
    type CLRN = utils::NormalizationConfig;
    type CPOOL = utils::PoolingConfig;

    fn init_nn() { let _ = CUDNN.id_c(); }
    fn device(&self) -> &DeviceType { self.device() }
}

impl_convolution_for_cuda_backend!(f32, ::cudnn::utils::DataType::Float);
impl_ops_sigmoid_for!(f32, Backend<Cuda>);
impl_ops_relu_for!(f32, Backend<Cuda>);
impl_ops_tanh_for!(f32, Backend<Cuda>);
impl_ops_softmax_for!(f32, Backend<Cuda>);
impl_ops_lrn_for!(f32, Backend<Cuda>);
impl_ops_pooling_for!(f32, Backend<Cuda>);

impl NN<f64> for Backend<Cuda> {
    type CC = utils::ConvolutionConfig;
    type CLRN = utils::NormalizationConfig;
    type CPOOL = utils::PoolingConfig;

    fn init_nn() { let _ = CUDNN.id_c(); }
    fn device(&self) -> &DeviceType { self.device() }
}

impl_convolution_for_cuda_backend!(f64, ::cudnn::utils::DataType::Double);
impl_ops_sigmoid_for!(f64, Backend<Cuda>);
impl_ops_relu_for!(f64, Backend<Cuda>);
impl_ops_tanh_for!(f64, Backend<Cuda>);
impl_ops_softmax_for!(f64, Backend<Cuda>);
impl_ops_lrn_for!(f64, Backend<Cuda>);
impl_ops_pooling_for!(f64, Backend<Cuda>);