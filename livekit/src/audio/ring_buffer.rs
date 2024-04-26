pub struct Fixed<T: Default + Copy> {
    data: Box<[T]>,
    write_pos: usize,
    read_pos: usize,
    same_wrap: bool,
}

impl<T: Default + Copy> Fixed<T> {
    pub fn new(len: usize) -> Self {
        Self {
            data: vec![T::default(); len].into_boxed_slice(),
            write_pos: 0,
            read_pos: 0,
            same_wrap: true,
        }
    }

    pub fn write(&mut self, data: &[T]) -> usize {
        let free = self.available_write();
        let write = std::cmp::min(free, data.len());
        let margin = self.data.len() - self.write_pos;

        let mut n = write;
        if write > margin {
            self.data[0..margin].copy_from_slice(&data[0..margin]);
            self.write_pos = 0;
            self.same_wrap = false;
            n -= margin;
        }

        self.write_pos += n;
        write
    }

    pub fn read(&mut self, len: usize, dst: &mut [T]) -> usize {
        let mut index1 = 0;
        let mut len1 = 0;
        let mut index2 = 0;
        let mut len2 = 0;
        let read = self.read_regions(len, &mut index1, &mut len1, &mut index2, &mut len2);
        self.move_read_ptr(read);

        if len2 > 0 {
            // borrow from dst
            dst[0..len1].clone_from_slice(&self.data[index1..index1 + len1]);
            dst[len1..len].clone_from_slice(&self.data[index2..index2 + len2]);
        } else {
            // borrow from self.data
            dst[0..len].clone_from_slice(&self.data[index1..index1 + len1]);
        }

        read
    }

    fn move_read_ptr(&mut self, mut len: usize) -> usize {
        let free = self.available_write();
        let read = self.available_read();

        if len > read {
            len = read;
        }

        if len > free {
            len = free;
        }

        let mut read_pos = self.read_pos as isize;
        let data_len = self.data.len() as isize;
        read_pos += len as isize;
        if read_pos >= data_len {
            read_pos -= data_len;
            self.same_wrap = true;
        }

        if read_pos < 0 {
            read_pos += data_len;
            self.same_wrap = false;
        }

        self.read_pos = read_pos as usize;
        len
    }

    fn read_regions(
        &self,
        len: usize,
        index1: &mut usize,
        len1: &mut usize,
        index2: &mut usize,
        len2: &mut usize,
    ) -> usize {
        let readable = self.available_read();
        let read = std::cmp::min(readable, len);
        let margin = self.data.len() - self.read_pos;

        if read > margin {
            // Data is not contiguous
            *index1 = self.read_pos;
            *len1 = margin;
            *index2 = 0;
            *len2 = read - margin;
        } else {
            // Data is contiguous
            *index1 = self.read_pos;
            *len1 = read;
            *index2 = 0;
            *len2 = 0;
        }

        read
    }

    fn available_read(&self) -> usize {
        if self.same_wrap {
            self.write_pos - self.read_pos
        } else {
            self.data.len() - self.read_pos + self.write_pos
        }
    }

    fn available_write(&self) -> usize {
        self.data.len() - self.write_pos
    }
}
