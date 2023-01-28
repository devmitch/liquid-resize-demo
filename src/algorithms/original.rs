use std::cmp::min;

pub struct OriginalAlgo {
    pixels: Vec<[u8; 3]>,
    width: u32, // maybe have both of these as usize so we save a bunch of casts
    height: u32,
}

impl OriginalAlgo {
    pub fn new(pixels: Vec<[u8; 3]>, width: u32, height: u32) -> Self {
        Self {
            pixels,
            width,
            height,
        }
    }

    #[inline]
    fn index_of(&self, row: u32, col: u32) -> usize {
        (self.width * row + col) as usize
    }

    pub fn remove_vertical_seam(&mut self) -> Vec<u32> {
        let energy_matrix = self.calculate_energy_matrix();
        let mut dp: Vec<u32> = Vec::with_capacity((self.width * self.height) as usize);
        dp.extend(&energy_matrix[0..self.width as usize]);
        for row in 1..self.height {
            let left_dp = energy_matrix[self.index_of(row, 0)]
                + min(dp[self.index_of(row - 1, 0)], dp[self.index_of(row - 1, 1)]);
            dp.push(left_dp);
            for col in 1..self.width - 1 {
                let dp_val = energy_matrix[self.index_of(row, col)]
                    + min(
                        dp[self.index_of(row - 1, col - 1)],
                        min(
                            dp[self.index_of(row - 1, col)],
                            dp[self.index_of(row - 1, col + 1)],
                        ),
                    );
                dp.push(dp_val);
            }
            let right_dp = energy_matrix[self.index_of(row, self.width - 1)]
                + min(
                    dp[self.index_of(row - 1, self.width - 1)],
                    dp[self.index_of(row - 1, self.width - 2)],
                );
            dp.push(right_dp);
        }
        // need to traverse back up
        // [x, y, z] - remove (1, x), (2, y), (3, z), ...
        let mut to_remove: Vec<u32> = Vec::with_capacity(self.height as usize);
        let (mut lo, mut hi) = (
            self.index_of(self.height - 1, 0),
            self.index_of(self.height - 1, self.width - 1),
        );
        for row in (0..self.height).rev() {
            let new_idx = dp[lo..=hi]
                .iter()
                .enumerate()
                .min_by_key(|(_i, en)| *en)
                .map(|(i, _en)| (lo + i) as u32)
                .unwrap();
            to_remove.push(new_idx);
            if row != 0 {
                (lo, hi) = if new_idx == self.index_of(row, 0) as u32 {
                    (self.index_of(row - 1, 0), self.index_of(row - 1, 1))
                } else if new_idx == self.index_of(row, self.width - 1) as u32 {
                    (
                        self.index_of(row - 1, self.width - 2),
                        self.index_of(row - 1, self.width - 1),
                    )
                } else {
                    (
                        (new_idx - self.width - 1) as usize,
                        (new_idx - self.width + 1) as usize,
                    )
                }
            }
        }
        to_remove.reverse();
        let mut k = 0;
        self.pixels = self
            .pixels
            .iter()
            .enumerate()
            .filter(|(i, _pix)| {
                if k != to_remove.len() && *i == to_remove[k] as usize {
                    k += 1;
                    false
                } else {
                    true
                }
            })
            .map(|(_i, x)| *x)
            .collect();
        self.width -= 1;
        to_remove
    }

    fn calculate_energy_matrix(&self) -> Vec<u32> {
        let mut ret: Vec<u32> = Vec::with_capacity((self.width * self.height) as usize);
        for row in 0..self.height {
            for col in 0..self.width {
                // grim
                let left = if col == 0 {
                    self.pixels[(self.width * row + col) as usize]
                } else {
                    self.pixels[(self.width * row + col - 1) as usize]
                }
                .map(|x| x as i32);

                let right = if col == self.width - 1 {
                    self.pixels[(self.width * row + col) as usize]
                } else {
                    self.pixels[(self.width * row + col + 1) as usize]
                }
                .map(|x| x as i32);

                let x_diff = (left[0] - right[0]).pow(2)
                    + (left[1] - right[1]).pow(2)
                    + (left[2] - right[2]).pow(2);

                let above = if row == 0 {
                    self.pixels[(self.width * row + col) as usize]
                } else {
                    self.pixels[(self.width * (row - 1) + col) as usize]
                }
                .map(|x| x as i32);

                let below = if row == self.height - 1 {
                    self.pixels[(self.width * row + col) as usize]
                } else {
                    self.pixels[(self.width * (row + 1) + col) as usize]
                }
                .map(|x| x as i32);

                let y_diff = (above[0] - below[0]).pow(2)
                    + (above[1] - below[1]).pow(2)
                    + (above[2] - below[2]).pow(2);
                let energy = x_diff + y_diff;
                ret.push(energy as u32);
            }
        }
        ret
    }
}
