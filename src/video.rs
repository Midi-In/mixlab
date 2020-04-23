use std::sync::Arc;

use num_rational::Rational64;
use mixlab_codec::avc::AvcFrame;

#[derive(Debug)]
pub struct Frame {
    pub specific: AvcFrame,

    // frame duration in fractional seconds, possibly an estimate if frame
    // duration information is not available:
    pub duration_hint: Rational64,

    // points to any key frame that may be necessary to decode this frame
    pub key_frame: Option<Arc<Frame>>,
}

impl Frame {
    pub fn is_key_frame(&self) -> bool {
        self.specific.frame_type.is_key_frame()
    }

    pub fn id(self: &Arc<Frame>) -> FrameId {
        FrameId(self.clone())
    }
}

pub struct FrameId(Arc<Frame>);

impl PartialEq for FrameId {
    fn eq(&self, other: &Self) -> bool {
        let self_id = &*self.0 as *const Frame;
        let other_id = &*other.0 as *const Frame;

        self_id == other_id
    }
}
