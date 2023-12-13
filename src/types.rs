//! Protocol Buffers well-known wrapper types.
//!
//! This module provides implementations of `Message` for Rust standard library types which
//! correspond to a Protobuf well-known wrapper type. The remaining well-known types are defined in
//! the `bilrost-types` crate in order to avoid a cyclic dependency between `bilrost` and
//! `bilrost-build`.

use alloc::string::String;
use alloc::vec::Vec;

use ::bytes::{Buf, BufMut, Bytes};

use crate::{
    encoding::{
        bool, bytes, float32, float64, sint32, sint64, skip_field, string, uint32, uint64,
        Capped, DecodeContext, TagMeasurer, TagWriter, WireType,
    },
    DecodeError, Message,
};

/// `google.protobuf.BoolValue`
impl Message for bool {
    fn encode_raw<B: BufMut>(&self, buf: &mut B) {
        if *self {
            bool::encode(1, self, buf, &mut TagWriter::new());
        }
    }

    fn merge_field<B: Buf>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if tag == 1 {
            bool::merge(wire_type, self, buf, ctx)
        } else {
            skip_field(wire_type, buf)
        }
    }

    fn encoded_len(&self) -> usize {
        if *self {
            2
        } else {
            0
        }
    }

    fn clear(&mut self) {
        *self = false;
    }
}

/// `google.protobuf.UInt32Value`
impl Message for u32 {
    fn encode_raw<B: BufMut>(&self, buf: &mut B) {
        if *self != 0 {
            uint32::encode(1, self, buf, &mut TagWriter::new())
        }
    }

    fn merge_field<B: Buf>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if tag == 1 {
            uint32::merge(wire_type, self, buf, ctx)
        } else {
            skip_field(wire_type, buf)
        }
    }

    fn encoded_len(&self) -> usize {
        if *self != 0 {
            uint32::encoded_len(1, self, &mut TagMeasurer::new())
        } else {
            0
        }
    }

    fn clear(&mut self) {
        *self = 0;
    }
}

/// `google.protobuf.UInt64Value`
impl Message for u64 {
    fn encode_raw<B: BufMut>(&self, buf: &mut B) {
        if *self != 0 {
            uint64::encode(1, self, buf, &mut TagWriter::new())
        }
    }

    fn merge_field<B: Buf>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if tag == 1 {
            uint64::merge(wire_type, self, buf, ctx)
        } else {
            skip_field(wire_type, buf)
        }
    }

    fn encoded_len(&self) -> usize {
        if *self != 0 {
            uint64::encoded_len(1, self, &mut TagMeasurer::new())
        } else {
            0
        }
    }

    fn clear(&mut self) {
        *self = 0;
    }
}

/// `google.protobuf.Int32Value`
impl Message for i32 {
    fn encode_raw<B: BufMut>(&self, buf: &mut B) {
        if *self != 0 {
            sint32::encode(1, self, buf, &mut TagWriter::new())
        }
    }

    fn merge_field<B: Buf>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if tag == 1 {
            sint32::merge(wire_type, self, buf, ctx)
        } else {
            skip_field(wire_type, buf)
        }
    }

    fn encoded_len(&self) -> usize {
        if *self != 0 {
            sint32::encoded_len(1, self, &mut TagMeasurer::new())
        } else {
            0
        }
    }

    fn clear(&mut self) {
        *self = 0;
    }
}

/// `google.protobuf.Int64Value`
impl Message for i64 {
    fn encode_raw<B: BufMut>(&self, buf: &mut B) {
        if *self != 0 {
            sint64::encode(1, self, buf, &mut TagWriter::new())
        }
    }

    fn merge_field<B: Buf>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if tag == 1 {
            sint64::merge(wire_type, self, buf, ctx)
        } else {
            skip_field(wire_type, buf)
        }
    }

    fn encoded_len(&self) -> usize {
        if *self != 0 {
            sint64::encoded_len(1, self, &mut TagMeasurer::new())
        } else {
            0
        }
    }

    fn clear(&mut self) {
        *self = 0;
    }
}

/// `google.protobuf.FloatValue`
impl Message for f32 {
    fn encode_raw<B: BufMut>(&self, buf: &mut B) {
        if *self != 0.0 {
            float32::encode(1, self, buf, &mut TagWriter::new())
        }
    }

    fn merge_field<B: Buf>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if tag == 1 {
            float32::merge(wire_type, self, buf, ctx)
        } else {
            skip_field(wire_type, buf)
        }
    }

    fn encoded_len(&self) -> usize {
        if *self != 0.0 {
            float32::encoded_len(1, self, &mut TagMeasurer::new())
        } else {
            0
        }
    }

    fn clear(&mut self) {
        *self = 0.0;
    }
}

/// `google.protobuf.DoubleValue`
impl Message for f64 {
    fn encode_raw<B: BufMut>(&self, buf: &mut B) {
        if *self != 0.0 {
            float64::encode(1, self, buf, &mut TagWriter::new())
        }
    }

    fn merge_field<B: Buf>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if tag == 1 {
            float64::merge(wire_type, self, buf, ctx)
        } else {
            skip_field(wire_type, buf)
        }
    }

    fn encoded_len(&self) -> usize {
        if *self != 0.0 {
            float64::encoded_len(1, self, &mut TagMeasurer::new())
        } else {
            0
        }
    }

    fn clear(&mut self) {
        *self = 0.0;
    }
}

/// `google.protobuf.StringValue`
impl Message for String {
    fn encode_raw<B: BufMut>(&self, buf: &mut B) {
        if !self.is_empty() {
            string::encode(1, self, buf, &mut TagWriter::new())
        }
    }

    fn merge_field<B: Buf>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if tag == 1 {
            string::merge(wire_type, self, buf, ctx)
        } else {
            skip_field(wire_type, buf)
        }
    }

    fn encoded_len(&self) -> usize {
        if !self.is_empty() {
            string::encoded_len(1, self, &mut TagMeasurer::new())
        } else {
            0
        }
    }

    fn clear(&mut self) {
        self.clear();
    }
}

/// `google.protobuf.BytesValue`
impl Message for Vec<u8> {
    fn encode_raw<B: BufMut>(&self, buf: &mut B) {
        if !self.is_empty() {
            bytes::encode(1, self, buf, &mut TagWriter::new())
        }
    }

    fn merge_field<B: Buf>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if tag == 1 {
            bytes::merge(wire_type, self, buf, ctx)
        } else {
            skip_field(wire_type, buf)
        }
    }

    fn encoded_len(&self) -> usize {
        if !self.is_empty() {
            bytes::encoded_len(1, self, &mut TagMeasurer::new())
        } else {
            0
        }
    }

    fn clear(&mut self) {
        self.clear();
    }
}

/// `google.protobuf.BytesValue`
impl Message for Bytes {
    fn encode_raw<B: BufMut>(&self, buf: &mut B) {
        if !self.is_empty() {
            bytes::encode(1, self, buf, &mut TagWriter::new())
        }
    }

    fn merge_field<B: Buf>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if tag == 1 {
            bytes::merge(wire_type, self, buf, ctx)
        } else {
            skip_field(wire_type, buf)
        }
    }

    fn encoded_len(&self) -> usize {
        if !self.is_empty() {
            bytes::encoded_len(1, self, &mut TagMeasurer::new())
        } else {
            0
        }
    }

    fn clear(&mut self) {
        self.clear();
    }
}

/// `google.protobuf.Empty`
impl Message for () {
    fn encode_raw<B: BufMut>(&self, _buf: &mut B) {}

    fn merge_field<B: Buf>(
        &mut self,
        _tag: u32,
        wire_type: WireType,
        buf: &mut Capped<B>,
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        skip_field(wire_type, buf)
    }

    fn encoded_len(&self) -> usize {
        0
    }

    fn clear(&mut self) {}
}
