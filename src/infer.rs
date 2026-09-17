use std::path::Path;
use std::sync::Mutex;

use ndarray::ArrayD;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::{Session, SessionInputValue};
use ort::value::Tensor;

pub struct Model {
    session: Mutex<Session>,
}

pub enum Input {
    F32(ArrayD<f32>),
    I64(ArrayD<i64>),
}

impl From<ArrayD<f32>> for Input {
    fn from(array: ArrayD<f32>) -> Self {
        Input::F32(array)
    }
}

impl From<ArrayD<i64>> for Input {
    fn from(array: ArrayD<i64>) -> Self {
        Input::I64(array)
    }
}

impl Model {

    pub fn load(path: &Path) -> Option<Model> {
        let built = (|| {
            let mut builder = Session::builder()?
                .with_optimization_level(GraphOptimizationLevel::Level3)?
                .with_intra_threads(threads())?;
            builder.commit_from_file(path)
        })();
        match built {
            Ok(session) => Some(Model { session: Mutex::new(session) }),
            Err(err) => {
                log::warn!("{}: {err}", path.display());
                None
            }
        }
    }

    pub fn run(&self, inputs: Vec<Input>) -> Result<Vec<ArrayD<f32>>, ort::Error> {
        let values = inputs
            .into_iter()
            .map(|input| match input {
                Input::F32(array) => Tensor::from_array(array).map(SessionInputValue::from),
                Input::I64(array) => Tensor::from_array(array).map(SessionInputValue::from),
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut session = self.session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let outputs = session.run(&values[..])?;
        (0..outputs.len())
            .map(|index| outputs[index].try_extract_array::<f32>().map(|view| view.to_owned()))
            .collect()
    }
}

fn threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
}

#[cfg(test)]
mod tests {

    #[test]
    #[ignore]
    fn the_other_two_models() {
        if let Ok(path) = std::env::var("FACES") {
            let photo = image::open(path).unwrap().to_rgb8();
            let started = std::time::Instant::now();
            let faces = crate::cull::faces::detect(&photo);
            println!("faces: {:?} in {:?}", faces.as_ref().map(|f| f.len()), started.elapsed());
            for face in faces.unwrap_or_default() {
                println!("  {face:?}");
            }
        }

        if let Ok(path) = std::env::var("RAF") {
            for one in path.split(':') {
                let image = crate::io::raw::load_scaled(std::path::Path::new(one), 640).unwrap();
                let faces = crate::cull::faces::detect(&image);
                println!("faces in {one} at the cull's size: {:?}", faces.map(|f| f.len()));
            }
        }
        if let Ok(path) = std::env::var("BIRD") {
            let photo = image::open(path).unwrap().to_rgb8();
            let started = std::time::Instant::now();
            let embedding = crate::render::sam::encode(&photo).expect("sam installed");
            println!("sam encode in {:?}", started.elapsed());
            let started = std::time::Instant::now();
            let alpha = embedding.at(0.565, 0.34).expect("a mask");
            let share: f64 = alpha.data.iter().map(|v| *v as f64).sum::<f64>() / alpha.data.len() as f64;
            println!("sam click on the bird: {:.2}% of the frame in {:?}", share * 100.0, started.elapsed());
        }
    }
}
