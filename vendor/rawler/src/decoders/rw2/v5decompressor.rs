use crate::{
  decoders::*,
  decompressors::decompress_chunked_fn,
  pumps::{BitPump, BitPumpLSB},
};

pub(crate) fn decode_panasonic_v5(buf: &[u8], width: usize, height: usize, bps: u32, dummy: bool) -> std::result::Result<PixU16, String> {

  const V5_BLOCK_SIZE: usize = 0x4000;

  const V5_SECTION_SPLIT_OFFSET: usize = 0x1FF8;

  const V5_BYTES_PER_PACKET: usize = 16;

  const V5_PACKETS_PER_BLOCK: usize = V5_BLOCK_SIZE / V5_BYTES_PER_PACKET;

  let pixels_per_packet = match bps {
    12 => 10,
    14 => 9,
    _ => unreachable!(),
  };

  let pixels_per_block = V5_PACKETS_PER_BLOCK * pixels_per_packet;

  log::debug!("RW2 V5 decoder: pixels per block: {}, bps: {}", pixels_per_block, bps);

  decompress_chunked_fn(
    width,
    height,
    pixels_per_block,
    dummy,
    &(|chunk, block_id| {

      let src = &buf[block_id * V5_BLOCK_SIZE..block_id * V5_BLOCK_SIZE + V5_BLOCK_SIZE];

      let mut swapped = Vec::with_capacity(V5_BLOCK_SIZE);
      swapped.extend_from_slice(&src[V5_SECTION_SPLIT_OFFSET..]);
      swapped.extend_from_slice(&src[..V5_SECTION_SPLIT_OFFSET]);

      for (out, bytes) in chunk.chunks_exact_mut(pixels_per_packet).zip(swapped.chunks_exact(V5_BYTES_PER_PACKET)) {

        let mut pump = BitPumpLSB::new(bytes);
        out.iter_mut().for_each(|p| *p = pump.get_bits(bps) as u16);
      }
    }),
  )
}
