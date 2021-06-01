use super::*;

pub struct DfuSync<'mem, IO: DfuIo<Read = (), Write = usize>> {
    sans_io: DfuSansIo<'mem, IO>,
}

impl<'mem, IO: DfuIo<Read = (), Write = usize>> DfuSync<'mem, IO> {
    pub fn new(io: IO, memory_layout: &'mem memory_layout::mem, transfer_size: u32) -> Self {
        Self {
            sans_io: DfuSansIo::new(io, memory_layout, transfer_size),
        }
    }
}

impl<'mem, IO: DfuIo<Read = (), Write = usize>> DfuSync<'mem, IO> {
    // TODO
}
