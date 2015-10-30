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

#[derive(Clone,PartialEq,Eq)]
pub enum BO {
  BigEndian,
  LittleEndian,
}

impl ByteBuffer {
  pub fn allocate(capacity: usize) -> Self {
    ByteBuffer {
      buf: Rc::new(RawVec::with_capacity(capacity)),
      pos: 0,
      limit: capacity - 1,
      mark: None,
      offset: 0,
      byte_order: BO::BigEndian,
    }
  }
}

pub trait Buffer {
  fn cap(&self) -> usize;
  fn get_pos(&self) -> usize;
  fn set_pos(&mut self, newpos: usize) -> Result<&mut Self, &str>; 
  fn get_lim(&self) -> usize;
  fn set_lim(&mut self, newlim: usize) -> Result<&mut Self, &str>; 
  fn mark(&mut self) -> &mut Self;
  fn reset(&mut self) -> Result<&mut Self, &str>;
  fn clear(&mut self) -> &mut Self;
  fn flip(&mut self) -> &mut Self;
  fn rewind(&mut self) -> &mut Self;
  fn remaining(&self) -> usize;
  fn has_remaining(&self) -> bool; 
  fn next_index(&mut self) -> Result<usize, &str>; 
  fn get_order(&self) -> BO;
  fn set_order(&mut self, bo: BO) -> &mut Self;
}

impl Buffer for ByteBuffer {
  fn cap(&self) -> usize { self.buf.cap() - self.offset }

  fn get_pos(&self) -> usize { self.pos - self.offset }
  fn set_pos(&mut self, newpos: usize) -> Result<&mut Self, &str>  {
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
  fn set_lim(&mut self, newlim: usize) -> Result<&mut Self, &str>  {
    if newlim >= self.cap() { 
      return Err("You cannot set the limit beyond the buffer capacity!!");
    }     

    self.limit = newlim + self.offset;
    match self.mark {
      None => (),
      Some(mark) => if mark > self.limit { self.mark.take(); }
    } 

    Ok(self)
  }
  
  fn mark(&mut self) -> &mut Self {
    self.mark = Some(self.pos); self
  }
  fn reset(&mut self) -> Result<&mut Self, &str> {
    match self.mark {
      None => return Err("Mark has not been set yet."),
      Some(mark) => self.pos = mark,
    }
    Ok(self)
  }

  // This method does not actually erase the data in the buffer. 
  fn clear(&mut self) -> &mut Self {
    self.set_pos(0);
    let cap = self.cap();
    self.set_lim(cap - 1);
    self.mark.take();
    self  
  }

  fn flip(&mut self) -> &mut Self {
    self.limit = self.get_pos();
    self.set_pos(0);
    self.mark.take();
    self
  }

  fn rewind(&mut self) -> &mut Self {
    self.set_pos(0);
    self.mark.take();
    self
  }

  fn remaining(&self) -> usize { self.get_lim() - self.get_pos() + 1 }
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

  fn get_order(&self) -> BO { self.byte_order.clone() } 
  fn set_order(&mut self, bo: BO) -> &mut Self { 
    self.byte_order = bo; self
  }
   
}

impl ByteBuffer {
  pub fn wrap(bytes: &[u8]) -> ByteBuffer {
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

  pub fn put(&mut self, byte: u8) -> &mut Self {
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

  pub fn put_bytes(&mut self, bytes: &[u8]) -> Result<&mut Self, &str> {
    if bytes.len() > self.remaining() {
      return Err("You can't write bytes beyond the limit!!");
    }
    for b in bytes { self.put(*b); }
    Ok(self)
  }

  pub fn get(&mut self) -> u8 {
    unsafe {
      std::ptr::read((*self.buf).ptr().offset(
                       match self.next_index() {
                         Err(_) => self.pos as isize,
                         Ok(pos) => (pos + self.offset) as isize
                       }))
    }
  }
  
  pub fn slice(&self) -> ByteBuffer {
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
  
  pub fn get_bytes(&mut self, len: usize) -> Result<&[u8], &str> {
    if len > self.remaining() {
      return Err("The remaining buffer size is shorter than one you specified!");
    }

    let raw_result = unsafe {
      (*self.buf).ptr().offset(self.pos as isize)
    };

    for _ in 0..len { self.next_index(); }
    Ok(unsafe { std::slice::from_raw_parts(raw_result, len) })
  }
  
  pub fn vector(&self) -> Option<Vec<u8>> {
    unsafe {
      let content = self.buf.ptr();
      if content.is_null() { return None; }

      let result = std::slice::from_raw_parts(content, self.cap());   
      Some(result.clone().to_vec())
    }
  }  
}

macro_rules! define_trait {
  ( get, $($x:tt),* ) => ( 
    interpolate_idents! {
      pub trait Getter {
        $( fn [get_ $x] (&mut self) -> Option<$x>; )*
      }
    }
  );
  ( put, $($x:tt),* ) => ( 
    interpolate_idents! {
      pub trait Putter {
        $( fn [put_ $x] (&mut self, n: $x) -> Result<&mut Self, &str>; )*
      }
    }
  );
  ( vector, $($x:tt),* ) => (
    interpolate_idents! {
      pub trait AsVec {
        $( fn [as_ $x _vec] (&mut self) -> Option<Vec<$x>>; )*
      }
    }
  );  
}   

macro_rules! define_traits {
  ( $($x:ident),* ) => {
    define_trait!(get, $($x),*); 
    define_trait!(put, $($x),*); 
    define_trait!(vector, $($x),*); 
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
        $( fn [put_ $x] (&mut self, n: $x) -> Result<&mut Self, &str> {
             let len = std::mem::size_of::<$x>();
             let mut vec = Vec::<u8>::with_capacity(len);

             // oops!
             for _ in 0..len { vec.push(0); } 

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
  ( vector, $($x:tt),* ) => (
    interpolate_idents! {
      impl AsVec for ByteBuffer {
        $( fn [as_ $x _vec] (&mut self) -> Option<Vec<$x>> { 
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
                        let mut vec = Vec::<u8>::with_capacity(type_size);
                        // oh, Gee!!
                        for _ in 0..type_size { vec.push(0); }
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
    implement_trait!(vector, $($x),*); 
  }
}

implement_traits!(u16, u32, u64, i16, i32, i64, f32, f64);

#[test]
fn test_buffer() {
  let mut buf = ByteBuffer::allocate(4);
  assert!( buf.cap() == 4 );
  assert!( buf.get_pos() == 0 );    
  assert!( buf.set_pos(1).unwrap()
              .get_pos() 
            == 1 );   
  assert!( buf.get_lim() == 3 );
  assert!( buf.set_lim(2).unwrap()
              .get_lim() 
            == 2 );
  assert!( buf.set_lim(10).is_err() );
  buf.mark();
  assert!( buf.mark == Some(1) );
  assert!( buf.set_pos(2).unwrap()
              .reset().unwrap()
              .get_pos() 
            == 1);
  assert!( buf.clear().get_pos() == 0 
            && buf.mark.is_none() 
            && buf.get_lim() == 3 );
  assert!( buf.set_pos(2).unwrap()
              .mark()
              .flip()
              .get_lim() == 2
            && buf.mark.is_none()
            && buf.get_pos() == 0 );
  assert!( buf.rewind()
              .get_pos() == 0
            && buf.mark.is_none() ); 
  assert!( buf.remaining() == 3 );
  assert!( buf.has_remaining() );
  assert!( buf.next_index()
              .unwrap() == 0
            && buf.has_remaining()
            && buf.remaining() == 2 
            && buf.get_pos() == 1 );
  assert!( buf.next_index()
              .unwrap() == 1
            && !buf.has_remaining() 
            && buf.remaining() == 1 
            && buf.get_pos() == 2
            && buf.get_lim() == 2 );
  assert!( buf.next_index().is_err() );
  assert!( buf.get_order() == BO::BigEndian );
  assert!( buf.set_order(BO::LittleEndian)
              .get_order() == BO::LittleEndian );
}

#[test]
fn test_bytebuffer() {
  let rawbuf = [3u8; 4];   
  let mut bb = ByteBuffer::wrap(&rawbuf);  
  assert!( bb.get() == 3 );
  assert!( bb.get() == 3 );
  assert!( bb.get() == 3 );
  assert!( bb.get() == 3 );
  assert!( bb.get() == 3 );
  assert!( bb.get_pos() == bb.get_lim() );
  assert!( bb.put(4)
             .get() == 4 ); 
  assert!( bb.get_pos() == bb.get_lim() );
  assert!( bb.put(5)
             .get() == 5 );
  let newbyte = [7u8; 4];
  assert!( bb.put_bytes(&newbyte)
             .is_err() );

  bb.clear();
  assert!( bb.put_bytes(&newbyte)
             .is_ok() ); 
  assert!( bb.get_pos() == bb.get_lim() );
  bb.clear();
  assert!( bb.get() == 7 );
  assert!( bb.get() == 7 );
  assert!( bb.get() == 7 );
  assert!( bb.get() == 7 );
  bb.clear();
  let mut bbslice = bb.slice();
  bbslice.put(1);
  assert!( bb.get() == 1 );
  bbslice.put(1);
  assert!( bb.get() == 1 );
  bbslice.put(1);
  bbslice.put(1);
  bb.clear();
  for i in bb.get_bytes(4).unwrap().iter() {
    assert!( *i == 1 );
  }
  assert!( bb.get_bytes(10).is_err() ); 
  bb.clear();
  let v = bb.vector().unwrap();
  for i in 0..bb.cap() {
    assert!( *v.get(i).unwrap() == bb.get() );
  }
}

#[test]
fn test_get_and_put() {
  let mut bb = ByteBuffer::allocate(16);
  bb.put_u16(10).unwrap().clear();
  assert!( bb.get_u16().unwrap() == 10 );
  bb.clear();
  bb.put_u32(70).unwrap().clear();
  assert!( bb.get_u32().unwrap() == 70 );
  bb.clear();
  bb.put_u64(7000).unwrap().clear();
  assert!( bb.get_u64().unwrap() == 7000 );
  bb.clear();
  bb.put_i16(999).unwrap().clear();
  assert!( bb.get_i16().unwrap() == 999 );
  bb.clear();
  bb.put_i32(77).unwrap().clear();
  assert!( bb.get_i32().unwrap() == 77 );
  bb.clear();
  bb.put_i64(66666).unwrap().clear();
  assert!( bb.get_i64().unwrap() == 66666 );
  bb.clear();
  bb.put_f32(77.777).unwrap().clear();
  assert!( bb.get_f32().unwrap() == 77.777 );
  bb.clear();
  bb.put_f64(66666.6666).unwrap().clear();
  assert!( bb.get_f64().unwrap() == 66666.6666 );
}

#[test]
fn test_vec() { 
  macro_rules! vtest {
    ( $x:tt, $y:expr ) => (
      interpolate_idents! { 
        {
          let tsize = std::mem::size_of::<$x>();
          let mut bb = ByteBuffer::allocate(tsize * 3);
          bb.[put_ $x]($y).unwrap() 
            .[put_ $x]($y).unwrap()
            .[put_ $x]($y).unwrap()
            .clear();
          let mut v = bb.[as_ $x _vec]().unwrap();
          let mut it = v.iter_mut();
          assert!( *it.next().unwrap() == $y );
          assert!( *it.next().unwrap() == $y );
          assert!( *it.next().unwrap() == $y );
        }
      }
    ) 
  }

  vtest!(u16, 15);
  vtest!(u32, 151);
  vtest!(u64, 7777777);
  vtest!(i16, 15);
  vtest!(i32, 666);
  vtest!(i64, 9999999);
  vtest!(f32, 666.666);
  vtest!(f64, 9999999.999999);
}
