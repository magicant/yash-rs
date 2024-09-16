// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki

#![no_std]
extern crate alloc;

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
