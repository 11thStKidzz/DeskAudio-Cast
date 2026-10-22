//! Encoders para AirPlay 2. De momento placeholder; la implementación ALAC
//! llega en el Hito 3/4 del plan.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EncodeError {
    #[error("not yet implemented")]
    NotImplemented,
}

pub trait Encoder {
    fn encode(&mut self, pcm: &[i16], out: &mut Vec<u8>) -> Result<(), EncodeError>;
}
