//! Provides the container of a Deep Learning Network
//!
//! Holds all the information about its Layers, how they are connected
//! and how the [forward][1] and [backward][2] steps should be
//! handeled and optimized (e.g. skipping layers).
//!
//! [1]: ./struct.Network.html#method.forward
//! [2]: ./struct.Network.html#method.backward
//!
//! If you are looking to train/test a network, [Solver][3] is usually a better
//! entry point.
//!
//! ## Development
//!
//! Currently only new networks can be created with [from_config][4].
//! In the future there should also be a way to load networks with saved
//! weights from a file.
//! [Issue #14][5].
//!
//! [3]: ../solver/index.html
//! [4]: #method.from_config
//! [5]: https://github.com/autumnai/leaf/issues/14
//! [6]: https://github.com/autumnai/leaf/issues/16
//!
//! ## Glossary
//! ### Input Layers / Blobs
//!
//! A input layer is the first layer of a network.</br>
//! During a forward step the data is put into the input layer,
//! passed through all the intermediate (hidden) layers and generates a
//! result in the output layer.
//!
//! The blobs in a input layer contain externally preprocessed data that has
//! been brought into a form suitable for consumption by a neural network.
use std::rc::Rc;
use co::IBackend;
use co::tensor::*;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use layer::{ILayer, Layer};
use layer::LayerConfig;
use util::{ArcLock, LayerOps, SolverOps};

#[derive(Debug)]
/// Defines a [Network][1] that contains the [Layers][2] and [Blobs][3] that store
/// the intermediate results between the layers which are generated by [forward][4]/[backward][5].
/// [1]: https://en.wikipedia.org/wiki/Artificial_neural_network
/// [2]: ../layer/struct.Layer.html
/// [3]: ../../phloem/blob/struct.Blob.html
/// [4]: ./struct.Network.html#method.forward
/// [5]: ./struct.Network.html#method.backward
///
/// It is also responsible for setting up the connections between the layers.
/// A Network is usually used together with a [Solver][6] to optimize the networks' weights.
///
/// [6]: ../solver/struct.Solver.html
pub struct Network<B: IBackend + LayerOps<f32>> {
    /// Identifies the Network
    ///
    /// The name is mainly used for logging purposes.
    pub name: String,
    layers: Vec<Layer<B>>,

    blobs_data: Vec<ArcLock<SharedTensor<f32>>>, // the blobs storing intermediate results between the layer.
    blobs_gradient: Vec<ArcLock<SharedTensor<f32>>>, // the blobs storing intermediate results between the layer.
    blob_names: Vec<String>,

    input_blobs_data: Vec<ArcLock<SharedTensor<f32>>>,
    input_blobs_gradient: Vec<ArcLock<SharedTensor<f32>>>,
    input_blob_names: Vec<String>,
    output_blobs_data: Vec<ArcLock<SharedTensor<f32>>>,
    output_blobs_gradient: Vec<ArcLock<SharedTensor<f32>>>,

    registry: HashMap<String, (ArcLock<SharedTensor<f32>>, ArcLock<SharedTensor<f32>>)>,

    weight_owners: Vec<Option<usize>>,
    weight_display_names: Vec<String>,
    weight_layer_indices: Vec<(usize, usize)>,
    weight_names_index: HashMap<String, usize>,

    /// Defines the [parameters/weights][1] of the network.
    /// [1]: https://en.wikipedia.org/wiki/Synaptic_weight
    ///
    /// Parameters are currently in the process of being renamed to weights throughout the codebase.
    /// [Issue #17](https://github.com/autumnai/leaf/issues/17)
    weights: Vec<ArcLock<SharedTensor<f32>>>,
    learnable_weights_data: Vec<ArcLock<SharedTensor<f32>>>,
    learnable_weights_gradient: Vec<ArcLock<SharedTensor<f32>>>,
    learnable_weight_ids: Vec<usize>,

    weights_lr: Vec<Option<f32>>,
    weights_weight_decay: Vec<Option<f32>>,
}

impl<B: IBackend + LayerOps<f32>> Default for Network<B> {
    fn default() -> Network<B> {
        Network {
            name: "".to_owned(),
            layers: vec![],

            blobs_data: vec![],
            blobs_gradient: vec![],
            blob_names: vec![],

            input_blobs_data: vec![],
            input_blobs_gradient: vec![],
            input_blob_names: vec![],
            output_blobs_data: vec![],
            output_blobs_gradient: vec![],

            registry: HashMap::new(),

            weight_owners: vec![],
            weight_display_names: vec![],
            weight_layer_indices: vec![],
            weight_names_index: HashMap::<String, usize>::new(),

            weights: vec![],
            learnable_weights_data: vec![],
            learnable_weights_gradient: vec![],
            learnable_weight_ids: vec![],

            weights_lr: vec![],
            weights_weight_decay: vec![],
        }
    }
}

impl<B: IBackend + LayerOps<f32> + 'static> Network<B> {
    /// Creates a Network from a [NetworkConfig][1].
    /// [1]: ./struct.NetworkConfig.html
    ///
    /// ## Examples
    ///
    /// ```
    /// # extern crate collenchyma;
    /// # extern crate leaf;
    ///
    /// # use leaf::network::*;
    /// # use collenchyma::prelude::*;
    /// # use std::rc::Rc;
    ///
    /// # #[cfg(feature="cuda")]
    /// # fn main() {
    /// // create backend
    /// let backend = Rc::new(Backend::<Cuda>::default().unwrap());
    /// // create network
    /// let cfg = NetworkConfig::default();
    /// Network::from_config(backend, &cfg);
    /// # }
    /// # #[cfg(not(feature="cuda"))]
    /// # fn main() {}
    /// ```
    pub fn from_config(backend: Rc<B>, param: &NetworkConfig) -> Network<B> {
        let mut network = Network::default();
        network.init(backend, param);
        network
    }

    /// Initializes a network.
    ///
    /// Sets up the whole structure of the network. It reads the supplied [NetworkConfig][1],
    /// appends the top and bottom blobs to each layer and determines if the backpropagation has
    /// to be executed for each blob and layer.
    ///
    /// [1]: ./struct.NetworkConfig.html
    fn init(&mut self, backend: Rc<B>, in_config: &NetworkConfig) {
        let config = in_config.clone();
        let mut registry = HashMap::<String, (ArcLock<SharedTensor<f32>>, ArcLock<SharedTensor<f32>>)>::new();
        let weight_registry = &mut HashMap::<String, (ArcLock<SharedTensor<f32>>, ArcLock<SharedTensor<f32>>, Option<f32>, Option<f32>)>::new();

        for (input_name, input_shape) in config.inputs.iter().zip(config.input_shapes.iter()) {
            self.init_input_blob(backend.clone(), &input_name, input_shape, &mut registry);
        }

        for layer_config in &config.layers {
            self.init_layer(backend.clone(), &layer_config, &mut registry, weight_registry);
        }

        // Go through the net backwards to determine which blobs contribute to the
        // loss.  We can skip backward computation for blobs that don't contribute
        // to the loss.
        // Also checks if all bottom blobs don't need backward computation (possible
        // because the skip_propagate_down config) and so we can skip backward
        // computation for the entire layer
        let blobs_under_loss = &mut HashSet::<String>::new();
        let blobs_skip_backp = &mut HashSet::<String>::new();
        for layer in &mut self.layers.iter_mut().rev() {
            layer.init_backprop( blobs_under_loss, blobs_skip_backp);
        }

        if config.force_backward {
            for layer in &mut self.layers {
                layer.init_force_backward();
            }
        }

        // In the end, all remaining blobs are considered output blobs.
        for (blob_name, blob) in registry.iter() {
            info!("This network produces output {}", blob_name);
            self.output_blobs_data.push(blob.0.clone());
            self.output_blobs_gradient.push(blob.1.clone());
        }

        self.share_weights();
        self.registry = registry;

        info!("Network initialization done.");
    }

    /// Initializes a single layer of the network.
    ///
    /// Appends [top][1] and [bottom blobs][2] to the [Layer][3]. Apart from explicitly named
    /// top blobs it will also append anonymous top blobs that are required by the specific
    /// [Layer implemenations][4]. It also sets up the [loss weights],
    /// and backpropagation flags.
    ///
    /// [1]: ../layer/index.html
    /// [2]: ../layer/index.html
    /// [3]: ../layer/struct.Layer.html
    /// [4]: ../layers/index.html
    fn init_layer(&mut self,
                  backend: Rc<B>,
                  layer_config: &LayerConfig,
                  registry: &mut HashMap<String, (ArcLock<SharedTensor<f32>>, ArcLock<SharedTensor<f32>>)>,
                  weight_registry: &mut HashMap<String, (ArcLock<SharedTensor<f32>>, ArcLock<SharedTensor<f32>>, Option<f32>, Option<f32>)>) {

        // Setup layer.
        if let Err(e) = layer_config.validate() {
            error!("{}", e);
        }

        info!("Creating Layer {}", layer_config.name.clone());
        let mut layer = Layer::from_config(backend, &layer_config);

        // Figure out this layer's input and output
        layer.connect(registry, weight_registry);
        for weight_data in &layer.weights_data {
            self.learnable_weights_data.push(weight_data.clone());
        }
        for weight_gradient in &layer.weights_gradient {
            self.learnable_weights_gradient.push(weight_gradient.clone());
        }

        self.layers.push(layer);
    }

    /// Share weights among multiple layers.
    ///
    /// Shared weights are usually used for [Siamese networks][1]
    ///
    /// [1]: http://citeseerx.ist.psu.edu/viewdoc/summary?doi=10.1.1.28.4792
    fn share_weights(&mut self) {
        // Caffe / not sure if ported correctly
        // for (int i = 0; i < params_.size(); ++i) {
        //     if (param_owners_[i] < 0) { continue; }
        //     params_[i]->ShareData(*params_[param_owners_[i]]);
        //     params_[i]->ShareDiff(*params_[param_owners_[i]]);
        // }
        for (i, _) in self.weights.clone().iter().enumerate() {
            if let Some(j) = self.weight_owners[i] {
                assert!(self.weights[i].read().unwrap().desc().size() ==
                        self.weights[j].read().unwrap().desc().size());
                self.weights[i] = self.weights[j].clone(); // sharing whole blob?
            }
        }
    }

    /// Initialize input blobs for the Network.
    ///
    /// Appends a input blob to the network, so the bottom-most [Layer][1] can
    /// [connect][2] to them.
    ///
    /// Used during initialization of the Network.
    /// [1]: ../layer/struct.Layer.html
    /// [2]: ../layer/struct.Layer.html#method.connect
    #[cfg_attr(lint, allow(ptr_arg))]
    fn init_input_blob(&mut self,
                  backend: Rc<B>,
                  blob_name: &str,
                  input_shape: &Vec<usize>,
                  registry: &mut HashMap<String, (ArcLock<SharedTensor<f32>>, ArcLock<SharedTensor<f32>>)> ) {

        if registry.contains_key(blob_name) {
            // If we are not doing in-place computation but have duplicated blobs, raise an
            // error.
            error!("Top blob {} produced by multiple sources.", blob_name);
            return
        } else {
            info!("Input {} -> {}", self.input_blobs_data.len(), blob_name);

            let ibackend: Rc<IBackend<F=B::F>> = backend;
            let blob_data: ArcLock<SharedTensor<f32>> = Arc::new(RwLock::new(SharedTensor::new(ibackend.device(), input_shape).unwrap()));
            let blob_gradient: ArcLock<SharedTensor<f32>> = Arc::new(RwLock::new(SharedTensor::new(ibackend.device(), input_shape).unwrap()));
            let blob_id = self.blobs_data.len();
            self.blobs_data.push(blob_data.clone());
            self.blob_names.push(blob_name.to_owned());

            self.input_blobs_data.push(blob_data.clone());
            self.input_blob_names.push(blob_name.to_owned());
            registry.insert(blob_name.to_owned(), (blob_data, blob_gradient));
        }
    }

    /// Computes [forward][1] and [backward][2] step for the network and returns [the total loss.][3]
    /// [1]: #method.forward
    /// [2]: #method.backward
    /// [3]: http://caffe.berkeleyvision.org/tutorial/loss.html
    ///
    /// Used by the [Solver][4] to conveniently compute one [forward- and one backward-propagation
    /// step][5] together, which is all the network has to do while training it.
    ///
    /// [4]: ../solver/struct.Solver.html
    /// [5]: https://en.wikipedia.org/wiki/Backpropagation#Phase_1:_Propagation
    pub fn forward_backward(&mut self, bottom: &[ArcLock<SharedTensor<f32>>]) -> f32 {
        let loss = &mut 0f32;

        self.forward(bottom, loss);
        self.backward();

        *loss
    }

    /// Copies supplied [input Blobs][1] into the network, computes [forward step][2] for the
    /// network and returns [the output blobs.][3].
    /// [1]: ./index.html#input-layers--blobs
    /// [2]: https://en.wikipedia.org/wiki/Feedforward_neural_network
    /// [3]: http://caffe.berkeleyvision.org/tutorial/loss.html
    ///
    /// Does not actually copy data, only references to the input blobs.
    ///
    /// This is the go-to if you just want to feed data to your network and get the corresponding
    /// output.
    pub fn forward(&mut self, input: &[ArcLock<SharedTensor<f32>>], loss: &mut f32) -> &Vec<ArcLock<SharedTensor<f32>>> {
        for (i, inp) in input.iter().enumerate() {
            self.input_blobs_data[i] = inp.clone();
            for layer in &mut self.layers {
                for (blob_index, blob_name) in layer.input_blob_names().to_owned().iter().enumerate() {
                    if blob_name == &self.input_blob_names[i] {
                        layer.input_blobs_data[blob_index] = inp.clone();
                    }
                }
            }
        }

        self.forward_prefilled(Some(loss))
    }

    /// Computes [forward step][1] for a network whose [input blob][2] references have been set
    /// and returns [the output blobs.][3]
    /// [1]: https://en.wikipedia.org/wiki/Feedforward_neural_network
    /// [2]: ./index.html#input-layers--blobs
    /// [3]: http://caffe.berkeleyvision.org/tutorial/loss.html
    ///
    /// Can be used if you need more control over how to put data into the network (debugging),
    /// otherwise [forward][4] is the prefered method to forward through the whole network.
    ///
    /// [4]: #method.forward
    pub fn forward_prefilled(&mut self, loss: Option<&mut f32>) -> &Vec<ArcLock<SharedTensor<f32>>> {
        let end = self.layers.len();
        match loss {
            Some(loss_result) => {
                // not sure if loss_result will really be changed
                *loss_result = self.forward_from_to(0, end);
            }
            None => {
                self.forward_from_to(0, end);
            }
        }

        &self.output_blobs_data
    }

    /// Compute [forward step][1] for a part of (or the whole) network and returns the [total loss][2].
    /// [1]: https://en.wikipedia.org/wiki/Feedforward_neural_network
    /// [2]: http://caffe.berkeleyvision.org/tutorial/loss.html
    ///
    /// Computes the forward step from the layer with index `start` to the layer with index `end`
    /// and return the total [scalar loss][2] over all loss layers.
    ///
    /// If you want to compute a foward step for the whole network
    /// you should use [forward_prefilled][3].
    /// Computing a forward on a part of the network is usually only done for debugging purposes.
    ///
    /// [3]: #method.forward_prefilled
    pub fn forward_from_to(&mut self, start: usize, end: usize) -> f32 {
        assert!(end <= self.layers.len());

        let mut loss = 0f32;

        for i in start..end {
            loss += self.layers[i].forward();
            if i == (end - 1) {
                // synchronize after last layer
                self.layers[i].synchronize();
            }
        }
        debug!("LOSS {:?}", loss);

        loss
    }

    /// Computes a [backpropagation][1] step for the whole network using the currently set output blobs.
    /// [1]: https://en.wikipedia.org/wiki/Backpropagation
    ///
    /// Computes the backpropagation step for each layer of the Network using [backward_from_to][2].
    /// [2]: #method.backward_from_to
    ///
    /// Called directly only for debugging purposes.
    /// Backpropagating a network is only useful during training and handled by a [Solver][3]
    /// [3]: ../solver/index.html
    pub fn backward(&mut self) {
        let start = self.layers.len();
        debug!("BACKWARD NETWORK START: {:?}", &start);
        self.backward_input_from_to(start, 0);
        self.backward_parameters_from_to(start, 0);
    }

    /// TODO: Docs
    pub fn backward_input(&mut self) {
        let start = self.layers.len();
        self.backward_input_from_to(start, 0);
    }

    /// TODO: Docs
    pub fn backward_parameters(&mut self) {
        let start = self.layers.len();
        self.backward_parameters_from_to(start, 0);
    }

    /// Compute [backpropagation][1] step for a part of (or the whole) network.
    /// [1]: https://en.wikipedia.org/wiki/Backpropagation
    ///
    /// Computes the backpropagation step from the layer with index `start` to the layer with index `end`,
    /// skipping layers that have been flagged to be skipped (usually in [init_backprop][2]).
    /// [2]: #method.init_backprop
    ///
    /// If you want to compute a foward step for the whole network you should use [backward][3].
    /// Computing a backward on a part of the network is usually only done for debugging purposes.
    /// [3]: #method.backward
    pub fn backward_input_from_to(&mut self, start: usize, end: usize) {
        // assert!(start < self.layers.len());
        debug!("BACKWARD NETWORK LAYERS");
        for i in (end..start).rev() {
            debug!("BACKWARD NETWORK LAYER {:?}", &self.layers[i].name);
            self.layers[i].backward_input();
            if i == end {
                // synchronize after last layer
                self.layers[i].synchronize();
            }
        }
    }

    /// TODO: Docs
    pub fn backward_parameters_from_to(&mut self, start: usize, end: usize) {
        debug!("BACKWARD NETWORK LAYERS");
        for i in (end..start).rev() {
            debug!("BACKWARD NETWORK LAYER {:?}", &self.layers[i].name);
            self.layers[i].backward_parameters();
            if i == end {
                // synchronize after last layer
                self.layers[i].synchronize();
            }
        }
    }

    /// Clears the [weights][1] diffs and zero-inits them.
    /// [1]: https://en.wikipedia.org/wiki/Synaptic_weight
    ///
    /// The diffs for the weights accumulate over the backpropagation steps of
    /// a [Solver][2] minibatch and are cleared between each minibatch
    /// to start over with a clean slate.
    ///
    /// [2]: ../solver/struct.Solver.html
    pub fn clear_weight_diffs(&mut self) {
        for weight_gradient in &mut self.learnable_weights_gradient.iter() {
            let filler = ::weight::FillerType::Constant {
                value: 0f32
            };
            filler.fill(&mut weight_gradient.write().unwrap());
        }
    }
}

impl<B: IBackend + LayerOps<f32>> Network<B> {
    /// Updates the [weights][1] with the weight update computed by the [Solver][2].
    /// [1]: https://en.wikipedia.org/wiki/Synaptic_weight
    /// [2]: ../solver/struct.Solver.html
    ///
    /// Updating the weights is the last step of computing a [Solver][2] minibatch.
    /// The update value is computed in previous steps according to the [learning rate policy][3]
    ///
    /// [3]: ../solver/enum.LRPolicy.html
    pub fn update_weights<SolverB: IBackend + SolverOps<f32>>(&mut self, backend: &SolverB) {
        let mut shared_a = ::util::native_scalar(-1f32);
        let _ = shared_a.add_device(backend.device());
        shared_a.sync(backend.device()).unwrap();
        for (weight_gradient, weight_data) in self.learnable_weights_gradient.iter().zip(&mut self.learnable_weights_data) {
            weight_gradient.write().unwrap().sync(backend.device()).unwrap();
            weight_data.write().unwrap().sync(backend.device()).unwrap();
            backend.axpy_plain(&shared_a, &weight_gradient.read().unwrap(), &mut weight_data.write().unwrap()).unwrap();
            // weight_blob.write().unwrap().apply_diff(backend) // TODO: solver
        }
    }

    #[allow(missing_docs)]
    pub fn learnable_weight_data(&self) -> &Vec<ArcLock<SharedTensor<f32>>> {
        &self.learnable_weights_data
    }

    #[allow(missing_docs)]
    pub fn learnable_weight_gradients(&self) -> &Vec<ArcLock<SharedTensor<f32>>> {
        &self.learnable_weights_gradient
    }

    /// get the data associated with the provided tensor name
    pub fn get_data(&self, name: &str) -> ArcLock<SharedTensor<f32>> {
        self.registry.get(name).unwrap().0.clone()
    }

    #[allow(missing_docs)]
    pub fn weights_weight_decay(&self) -> &Vec<Option<f32>> {
        &self.weights_weight_decay
    }

    #[allow(missing_docs)]
    pub fn weights_lr(&self) -> &Vec<Option<f32>> {
        &self.weights_lr
    }
}

#[derive(Debug, Clone)]
/// Defines the configuration of a network.
///
/// TODO: [DOC] When and why would you use this?
/// TODO: [DOC] What is the purpose of this configuration type?
///
/// TODO: [DOC] <Now-What> Examples
pub struct NetworkConfig {
    /// Defines the name the network.
    pub name: String,

    /// Defines the names of the [input blobs][1].
    /// [1]: ./index.html#input-layers--blobs
    ///
    /// The input blobs are identified by name so they can be referenced as [input blobs][2]
    /// in a [LayerConfig][3].
    ///
    /// [2]: ../layer/index.html
    /// [3]: ../layer/struct.LayerConfig.html
    pub inputs: Vec<String>,

    /// Defines the [shape][1] of the [input blobs][2].
    /// [1]: ???
    /// [2]: ./index.html#input-layers--blobs
    ///
    /// The number of input_shapes supplied should match the number of inputs supplied.
    /// The shape of the input blobs has to be known so that the right connections to the
    /// upper layers can be set up.
    pub input_shapes: Vec<Vec<usize>>,

    /// Defines if the network will force every layer to do [backpropagation][1].
    /// [1]: https://en.wikipedia.org/wiki/Backpropagation
    ///
    /// If set to `false`, then the execution of backpropagation is determined automatically
    /// according to the net structure and learning rates.
    ///
    /// Default: `false`
    pub force_backward: bool,

    /// Defines the [state][1] of the network.
    /// [1]: ../struct.NetworkState.html
    ///
    /// Some layers may be included/excluded depending on this state and the states
    /// specified in the layers' include and exclude fields.
    pub state: NetworkState,

    /// Defines if the network will print debugging information about results
    ///
    /// Default: `false`
    pub debug_info: bool,

    /// Defines the layers of the network via [LayerConfig][1]s.
    /// [1]: ../layer/struct.LayerConfig.html
    pub layers: Vec<LayerConfig>,
}

impl Default for NetworkConfig {
    fn default() -> NetworkConfig {
        NetworkConfig {
            name: "".to_owned(),
            inputs: Vec::new(),
            input_shapes: Vec::new(),

            force_backward: false,
            debug_info: false,

            layers: Vec::new(),
            state: NetworkState::default(),
        }
    }
}

impl NetworkConfig {
    #[allow(missing_docs)]
    pub fn layer(&self, layer_id: usize) -> Option<&LayerConfig> {
        self.layers.get(layer_id)
    }

    /// Add layer at the end of the network.
    pub fn add_layer(&mut self, layer: LayerConfig) {
        self.layers.push(layer);
    }

    #[allow(missing_docs)]
    pub fn input(&self, input_id: usize) -> Option<&String> {
        self.inputs.get(input_id)
    }

    #[allow(missing_docs)]
    pub fn input_shape(&self, input_id: usize) -> Option<&Vec<usize>> {
        self.input_shapes.get(input_id)
    }

    /// Add a input to the network.
    pub fn add_input(&mut self, input_name: &str, shape: &[usize]) {
        self.inputs.push(input_name.to_owned());
        self.input_shapes.push(shape.to_owned());
    }
}

#[derive(Debug, Clone)]
/// Defines the state of a network.
pub struct NetworkState {
    /// Defines the current mode of the network.
    ///
    /// Default: Test
    pub mode: NetworkMode,
    /// TODO: [DOC] what does this do?
    /// TODO: [DOC] could it be of type usize?
    ///
    /// Default: 0
    pub level: isize,
    /// TODO: [DOC] what does this do?
    ///
    /// Default: vec![]
    pub stage: Vec<String>,
}

impl Default for NetworkState {
    fn default() -> NetworkState {
        NetworkState {
            mode: NetworkMode::Test,
            level: 0,
            stage: vec![],
        }
    }
}

#[derive(Debug, Copy, Clone)]
/// Defines the possible modes that a network can be in.
pub enum NetworkMode {
    #[allow(missing_docs)]
    Train,
    #[allow(missing_docs)]
    Test,
}
