//! Implementation of [`TaskManager`]
//!
//! It is only used to manage processes and schedule process based on ready queue.
//! Other CPU process monitoring functions are in Processor.

use super::TaskControlBlock;
use crate::sync::UPSafeCell;
use crate::config::BIG_STRIDE;
use alloc::vec::Vec;
use alloc::sync::Arc;
use lazy_static::*;
use core::cmp::Ordering;

pub struct TaskManager {
    ready_queue: Vec<Arc<TaskControlBlock>>,
}

// YOUR JOB: FIFO->Stride
/// A simple FIFO scheduler.
impl TaskManager {
    pub fn new() -> Self {
        Self {
            ready_queue: Vec::new(),
        }
    }
    /// Add process back to ready queue
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push(task);
    }
    /// Take a process out of the ready queue
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        if self.ready_queue.is_empty() {
            return None;
        }
        let mut min_i = 0;
        let mut min_pass = self.ready_queue[0].inner_exclusive_access().pass;
        for i in 0..self.ready_queue.len() {
            let pass = self.ready_queue[i].inner_exclusive_access().pass;
            if pass < min_pass {
                min_i = i;
                min_pass = pass;
            }
        }
        Some(self.ready_queue.swap_remove(min_i))
    }
}

#[derive(Copy, Clone)]
pub struct Pass(pub u64);

impl Pass {
    pub fn new() -> Self {
        Self(0)
    }
    pub fn step_by_prio(&mut self, priority: isize) {
        let stride = match BIG_STRIDE as u64 / priority as u64 {
            0 => 1,
            o => o,
        };
        self.0 += stride;
    }
}

impl PartialOrd for Pass {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let overflow = self.0.abs_diff(other.0) > BIG_STRIDE/2;
        let order = self.0 <= other.0;
        if order ^ overflow {
            Some(Ordering::Less)
        }
        else {
            Some(Ordering::Greater)
        }
    }
}

impl PartialEq for Pass {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}
lazy_static! {
    /// TASK_MANAGER instance through lazy_static!
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

pub fn add_task(task: Arc<TaskControlBlock>) {
    TASK_MANAGER.exclusive_access().add(task);
}

pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    let task = TASK_MANAGER.exclusive_access().fetch()?;
    {
        let mut task_inner = task.inner_exclusive_access();
        let priority = task_inner.priority;
        task_inner.pass.step_by_prio(priority);
        info!("fetch task with PID {}, pass {}", task.pid.0, task_inner.pass.0);
    }
    Some(task)
}
pub fn set_priority(task: &TaskControlBlock, priority: isize) -> isize{
    if priority < 2 {
        -1
    }
    else {
        let mut task_inner = task.inner_exclusive_access();
        task_inner.priority = priority;
        0
    }
}