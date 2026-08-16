
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AudioPacket {}

impl AudioPacket {
    #[allow(unused)]
    fn from_slice(slice: &[f32]) -> Self {
        todo!()
    }
    #[allow(unused)]
    fn to_slice(&self) -> &[f32] {
        todo!()
    }
}
