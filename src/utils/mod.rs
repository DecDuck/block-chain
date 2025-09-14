use alloc::borrow::ToOwned;
use mcproto_rs::{types::{BaseComponent, TextComponent}, SerializeResult, Serializer};

pub fn text(text: &'static str) -> TextComponent {
    TextComponent {
        text: text.to_owned(),
        base: BaseComponent {
            ..Default::default()
        },
    }
}


pub struct SliceSerializer<'a> {
    target: &'a mut [u8],
    at: usize,
}

impl<'a> Serializer for SliceSerializer<'a> {
    fn serialize_bytes(&mut self, data: &[u8]) -> SerializeResult {
        let end_at = self.at + data.len();
        if end_at >= self.target.len() {
            panic!(
                "cannot fit data in slice ({} exceeds length {} at {})",
                data.len(),
                self.target.len(),
                self.at
            );
        }

        (&mut self.target[self.at..end_at]).copy_from_slice(data);
        self.at = end_at;
        Ok(())
    }
}

impl<'a> SliceSerializer<'a> {
    pub fn create(target: &'a mut [u8]) -> Self {
        Self { target, at: 0 }
    }

    pub fn finish(self) -> &'a [u8] {
        &self.target[..self.at]
    }
}