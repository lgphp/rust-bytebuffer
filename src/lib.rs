#![feature(core, alloc, plugin, custom_derive, convert)]
#![plugin(interpolate_idents)]
extern crate core;
extern crate alloc;
extern crate byteorder;

use alloc::raw_vec::RawVec;
use std::rc::Rc;
use byteorder::{BigEndian, LittleEndian};
use byteorder::ByteOrder;

pub struct ByteBuffer {
  buf: Rc<RawVec<u8>>,
  pos: usize,
  limit: usize,
  mark: Option<usize>,
  offset: usize,
  byte_order: BO, 
}

#[derive(Clone)]
enum BO {
  BigEndian,
  LittleEndian,
}

impl ByteBuffer {
  fn allocate(capacity: usize) -> Self {
    ByteBuffer {
      buf: Rc::new(RawVec::with_capacity(capacity)),
      pos: 0,
      limit: capacity,
      mark: None,
      offset: 0,
      byte_order: BO::BigEndian,
    }
  }
}

trait Buffer {
  fn cap(&self) -> usize;
  fn get_pos(&self) -> usize;
  fn set_pos(&mut self, newpos: usize) -> Result<&Self, &str>; 
  fn get_lim(&self) -> usize;
  fn set_lim(&mut self, newlim: usize) -> Result<&Self, &str>; 
  fn mark(&mut self) -> &Self;
  fn reset(&mut self) -> Result<&Self, &str>;
  fn clear(&mut self) -> &Self;
  fn flip(&mut self) -> &Self;
  fn rewind(&mut self) -> &Self;
  fn remaining(&self) -> usize;
  fn has_remaining(&self) -> bool; 
  fn next_index(&mut self) -> Result<usize, &str>; 
  fn get_order<'a>(&'a self) -> &'a BO;
  fn set_order(&mut self, bo: BO) -> &Self;
}

impl Buffer for ByteBuffer {
  fn cap(&self) -> usize { self.buf.cap() - self.offset }

  fn get_pos(&self) -> usize { self.pos - self.offset }
  fn set_pos(&mut self, newpos: usize) -> Result<&Self, &str>  {
    if newpos > self.limit { 
      return Err("The new position is beyond the limit!!");
    }     

    self.pos = newpos + self.offset;
    match self.mark {
      None => (),
      Some(mark) => if mark > self.pos { self.mark.take(); }
    } 
    Ok(self)
  } 

  fn get_lim(&self) -> usize { self.limit - self.offset }
  fn set_lim(&mut self, newlim: usize) -> Result<&Self, &str>  {
    if newlim > self.cap() { 
      return Err("You cannot set the limit beyond the buffer capacity!!");
    }     

    self.limit = newlim + self.offset;
    match self.mark {
      None => (),
      Some(mark) => if mark > self.limit { self.mark.take(); }
    } 

    Ok(self)
  }
  
  fn mark(&mut self) -> &Self {
    self.mark = Some(self.pos); self
  }
  fn reset(&mut self) -> Result<&Self, &str> {
    match self.mark {
      None => return Err("Mark has not been set yet."),
      Some(mark) => self.pos = mark,
    }
    Ok(self)
  }

  // This method does not actually erase the data in the buffer. 
  fn clear(&mut self) -> &Self {
    self.set_pos(0);
    let cap = self.cap();
    self.set_lim(cap);
    self.mark.take();
    self  
  }

  fn flip(&mut self) -> &Self {
    self.limit = self.get_pos();
    self.set_pos(0);
    self.mark.take();
    self
  }

  fn rewind(&mut self) -> &Self {
    self.set_pos(0);
    self.mark.take();
    self
  }

  fn remaining(&self) -> usize { self.get_lim() - self.get_pos() }
  fn has_remaining(&self) -> bool { self.get_pos() < self.get_lim() }

  // You have to notice that next_index function returns the current position!! 
  fn next_index(&mut self) -> Result<usize, &str> {
    let pos = self.get_pos();
    if pos >= self.get_lim() { 
      return Err("The current position is the limit!!");
    }
    self.set_pos(pos + 1);
    Ok(pos)
  }   

  fn get_order<'a>(&'a self) -> &'a BO { &self.byte_order } 
  fn set_order(&mut self, bo: BO) -> &Self { 
    self.byte_order = bo; self
  }
   
}

impl ByteBuffer {
  fn wrap(bytes: &[u8]) -> ByteBuffer {
    let bb = ByteBuffer::allocate(bytes.len());
    let mut cnt = 0;  
    for b in bytes.into_iter() { 
      unsafe { 
        std::ptr::write((*bb.buf).ptr().offset(cnt as isize), 
                        b.clone());
      }   
      cnt += 1;
    }
    bb
  }

  fn put(&mut self, byte: u8) -> &Self {
    unsafe {
      std::ptr::write((*self.buf).ptr().offset(
                        match self.next_index() {
                          Err(_) => self.pos as isize,
                          Ok(pos) => (pos + self.offset) as isize,
                        }),
                      byte);
    }
    self
  }

  fn put_bytes(&mut self, bytes: &[u8]) -> Result<&Self, &str> {
    if bytes.len() > self.remaining() {
      return Err("You can't write bytes beyond the limit!!");
    }
    for b in bytes { self.put(*b); }
    Ok(self)
  }

  fn get(&mut self) -> u8 {
    unsafe {
      std::ptr::read((*self.buf).ptr().offset(
                       match self.next_index() {
                         Err(_) => self.pos as isize,
                         Ok(pos) => (pos + self.offset) as isize
                       }))
    }
  }
  
  fn slice(&self) -> ByteBuffer {
    let pos = self.get_pos();
    let lim = self.get_lim();
    assert!(pos <= lim);  
    let off = pos + self.offset;
    
    ByteBuffer {
      buf: self.buf.clone(), 
      pos: pos,
      limit: lim,
      mark: None,
      offset: off,
      byte_order: self.get_order().clone(),
    }
  } 
  
  fn get_bytes(&mut self, len: usize) -> Result<&[u8], &str> {
    if len > self.remaining() {
      return Err("The remaining buffer size is shorter than one you specified!");
    }

    let raw_result = unsafe {
      (*self.buf).ptr().offset(self.pos as isize)
    };
    Ok(unsafe { std::slice::from_raw_parts(raw_result, len) })
  }
}

macro_rules! define_trait {
  ( get, $($x:tt),* ) => ( 
    interpolate_idents! {
      trait Getter {
        $( fn [get_ $x] (&mut self) -> Option<$x>; )*
      }
    }
  );
  ( put, $($x:tt),* ) => ( 
    interpolate_idents! {
      trait Putter {
        $( fn [put_ $x] (&mut self, n: $x) -> Result<&Self, &str>; )*
      }
    }
  );
  ( slice, $($x:tt),* ) => (
    interpolate_idents! {
      trait AsSlice {
        $( fn [as_ $x _slice] (&mut self) -> Option<Vec<$x>>; )*
      }
    }
  );  
}   

macro_rules! define_traits {
  ( $($x:ident),* ) => {
    define_trait!(get, $($x),*); 
    define_trait!(put, $($x),*); 
    define_trait!(slice, $($x),*); 
  }
}

define_traits!(u16, u32, u64, i16, i32, i64, f32, f64);

macro_rules! implement_trait {
  ( get, $($x:tt),* ) => (
    interpolate_idents! {
      impl Getter for ByteBuffer {
        $( fn [get_ $x] (&mut self) -> Option<$x> {
             let len = std::mem::size_of::<$x>();
             let bytes = match self.get_bytes(len)  {
                           Err(_) => { return None; },
                           Ok(bytes) => bytes,
                         };
             let num = match self.byte_order {
                         BO::LittleEndian => LittleEndian::[read_ $x](bytes),
                         _ => BigEndian::[read_ $x](bytes),
                       };
             Some(num) 
        } )* 
      }
    }
  );
  ( put, $($x:tt),* ) => (
    interpolate_idents! {
      impl Putter for ByteBuffer {
        $( fn [put_ $x] (&mut self, n: $x) -> Result<&Self, &str> {
             let len = std::mem::size_of::<$x>();
             let mut vec = std::vec::Vec::with_capacity(len);
             let buf = vec.as_mut_slice();
             match self.byte_order {
               BO::LittleEndian => LittleEndian::[write_ $x](buf, n),
               _ => BigEndian::[write_ $x](buf, n),
             };
             self.put_bytes(buf)
        } )*
      }
    }
  );
  ( slice, $($x:tt),* ) => (
    interpolate_idents! {
      impl AsSlice for ByteBuffer {
        $( fn [as_ $x _slice] (&mut self) -> Option<Vec<$x>> { 
             let type_size = std::mem::size_of::<$x>();
             let len = match self.remaining() / type_size { 
                         0 => return None,
                         len => len * type_size,
                       };  
             let bytes = match self.get_bytes(len) {
                           Ok(bytes) => bytes.as_ptr() as *const $x,
                           _ => panic!("Something wrong would be happening!!"),
                         };

             let result: Vec<_> = 
               unsafe { 
                std::slice::from_raw_parts(bytes, len)
                  .iter()
                  .map(|&num| { 
                        let mut vec = Vec::with_capacity(len);
                        let buf = vec.as_mut_slice();
                        match self.byte_order {
                          BO::LittleEndian => 
                            LittleEndian::[write_ $x](buf, num),
                          _ => BigEndian::[write_ $x](buf, num),
                        };
                        *(buf.as_ptr() as *const $x)
                      })
                  .collect()
                };
             Some(result)
        } )*           
      }
    }
  ); 
} 

macro_rules! implement_traits {
  ( $($x:tt),* ) => { 
    implement_trait!(get, $($x),*); 
    implement_trait!(put, $($x),*); 
    implement_trait!(slice, $($x),*); 
  }
}

implement_traits!(u16, u32, u64, i16, i32, i64, f32, f64);

