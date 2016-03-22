//! A [Stochastic Gradient Descent with Momentum][1]
//! [1]: https://en.wikipedia.org/wiki/Stochastic_gradient_descent#Momentum
//!
//! Momentum in solving neural networks works similar to
//! they way it does in physics.
//! If you travel into a a direction with a high velocity,
//! it becomes very hard to change (or reverse)
//! the direction in which you are moving.
//!
//! Similarly when adjusting gradients during solving,
//! keeping a part of the previous gradient update can make solving faster,
//! since if you keep adjusting the gradients
//! into the same direction you will reach the optimum faster.
//! It also makes solving more stable.
use co::prelude::*;
use coblas::plugin::Copy;
use layer::*;
use solver::*;
use solvers::SGDSolver;
use std::rc::Rc;
use std::sync::{Arc, RwLock};
use util::*;

#[derive(Debug)]
/// Stochastic Gradient Descent with Momentum.
///
/// See [module description][1] for more information.
/// [1]: ./index.html
pub struct Momentum<SolverB: IBackend + SolverOps<f32>> {
    /// The gradient update from the previous iteration for each blob.
    history: Vec<ArcLock<SharedTensor<f32>>>,
    /// The backend used for computing the gradient.
    backend: Rc<SolverB>,

    lr_xx: Option<SharedTensor<f32>>,
    // momentum: SharedTensor<f32>,
}

impl<SolverB: IBackend + SolverOps<f32>> Momentum<SolverB> {
    /// Create a new SGD Momentum solver.
    ///
    /// Should not be called directly.
    /// Use [Solver::from_config][2] instead.
    ///
    /// [2]: ../../../solver/struct.Solver.html#method.from_config
    pub fn new(backend: Rc<SolverB>) -> Momentum<SolverB> {
        // println!("create solver");
        // let cuda = cuda_backend();
        // let lr = SharedTensor::<f32>::new(cuda.device(), &[1]).unwrap();
        // let mut momentum = SharedTensor::<f32>::new(cuda.device(), &[1]).unwrap();
        // println!("lr = {:?}", lr);
        // lr.add_device(cuda.device()).unwrap();
        // momentum.add_device(cuda.device()).unwrap();

        Momentum {
            history: Vec::new(),
            backend: backend,

            lr_xx: None,
            // momentum: momentum,
        }
    }

}

fn cuda_backend() -> Backend<Cuda> {
    let framework = Cuda::new();
    let hardwares = &framework.hardwares().to_vec();
    let backend_config = BackendConfig::new(framework, hardwares);
    Backend::new(backend_config).unwrap()
}

impl<B: IBackend + SolverOps<f32>, NetB: IBackend + LayerOps<f32> + 'static> SGDSolver<B, NetB> for Momentum<B> {
    fn compute_update_value(&mut self,
                            config: &SolverConfig,
                            weight_gradient: &ArcLock<SharedTensor<f32>>,
                            history_blob_id: usize,
                            global_lr: &f32,
                            blob_lr: &f32) {
        let op_backend = cuda_backend();

        if self.lr_xx.is_none() {
            let lr_xx = SharedTensor::<f32>::new(op_backend.device(), &[1]).unwrap();
            self.lr_xx = Some(lr_xx);
        }

        // let op_backend = native_backend();

        // println!("before: {:?}", self.lr);
        // let _ = self.lr.add_device(op_backend.device());
        // // self.lr.sync(op_backend.device()).unwrap();
        // println!("after: {:?}", self.lr);

        let history_blob = &self.history[history_blob_id];
        let local_momentum = config.momentum;
        let local_lr = global_lr * blob_lr;

        // let op_backend = native_backend();
        let backend = ISolver::<B, NetB>::backend(self);
        let device = IBackend::device(backend);

        println!("local_lr {}", local_lr);
        let mut lr_shared = native_scalar(local_lr);
        let _ = lr_shared.add_device(op_backend.device());
        lr_shared.sync(op_backend.device()).unwrap();

        let mut momentum_shared = native_scalar(local_momentum);
        let _ = momentum_shared.add_device(op_backend.device());
        momentum_shared.sync(op_backend.device()).unwrap();


        let _ = weight_gradient.write().unwrap().add_device(op_backend.device());
        weight_gradient.write().unwrap().sync(op_backend.device()).unwrap();
        let _ = history_blob.write().unwrap().add_device(op_backend.device());
        history_blob.write().unwrap().sync(op_backend.device()).unwrap();
        Axpby::<f32>::axpby_plain(&op_backend,
                                               &lr_shared,
                                               &weight_gradient.read().unwrap(),
                                               &momentum_shared,
                                               &mut history_blob.write().unwrap()).unwrap();

        op_backend.copy_plain(
            &history_blob.read().unwrap(), &mut weight_gradient.write().unwrap()).unwrap();
    }
}

impl_isolver_sgd!(Momentum<SolverB>);
