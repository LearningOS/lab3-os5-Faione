//! Process management syscalls

use crate::config::MAX_SYSCALL_NUM;
use crate::loader::get_app_data_by_name;
use crate::mm::{
    memeory_map, memeory_unmap, translated_refmut, translated_str, MapPermission, VirtAddr,
};
use crate::task::{
    add_task, current_task, current_user_token, exit_current_and_run_next,
    suspend_current_and_run_next, TaskStatus,
};
use crate::timer::get_time_us;
use alloc::sync::Arc;

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

#[derive(Clone, Copy)]
pub struct TaskInfo {
    pub status: TaskStatus,
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
    pub time: usize,
}

pub fn sys_exit(exit_code: i32) -> ! {
    debug!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    current_task().unwrap().pid.0 as isize
}

/// Syscall Fork which returns 0 for child process and child_pid for parent process
pub fn sys_fork() -> isize {
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

/// Syscall Exec which accepts the elf path
pub fn sys_exec(path: *const u8) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let task = current_task().unwrap();
        task.exec(data);
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    let task = current_task().unwrap();
    // find a child process

    // ---- access current TCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB lock exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after removing from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child TCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB lock automatically
}

// YOUR JOB: ???????????????????????? sys_get_time
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    let us = get_time_us();
    let token = current_user_token();
    *translated_refmut(token, ts) = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    0
}

// YOUR JOB: ???????????????????????? sys_task_info
pub fn sys_task_info(ti: *mut TaskInfo) -> isize {
    let task_info = {
        let task = current_task().unwrap();
        let inner = task.inner_exclusive_access();
        let time = match inner.task_status {
            TaskStatus::Zombie => inner.addtion_info.time,
            _ => get_time_us() - inner.addtion_info.time,
        };
        TaskInfo {
            status: inner.task_status,
            syscall_times: inner.addtion_info.syscall_times,
            time: time / 1_000,
        }
    };

    let token = current_user_token();
    *translated_refmut(token, ti) = task_info;
    0
}

// YOUR JOB: ??????sys_set_priority???????????????????????????
pub fn sys_set_priority(prio: isize) -> isize {
    if prio < 2 {
        -1
    } else {
        let task = current_task().unwrap();
        let mut inner = task.inner_exclusive_access();
        inner.priority.set_prio(prio as usize);
        prio
    }
}

// YOUR JOB: ????????????????????? sys_mmap ??? sys_munmap
pub fn sys_mmap(start: usize, len: usize, port: usize) -> isize {
    if (port & !0x7) != 0 || (port & 0x7) == 0 {
        return -1;
    }

    let len = ((len - 1) / 4096 + 1) * 4096;

    let start_va: VirtAddr = start.into();
    if start_va.page_offset() != 0 {
        return -1;
    }

    let end_va: VirtAddr = (start + len - 1).into();

    let mut map_perm = MapPermission::U;

    if port & 0x1 == 1 {
        map_perm |= MapPermission::R;
    };

    if (port >> 1) & 0x1 == 1 {
        map_perm |= MapPermission::W;
    };

    if (port >> 2) & 0x1 == 1 {
        map_perm |= MapPermission::X;
    };

    if let Err(err) = memeory_map(start_va, end_va, map_perm) {
        error!(" sys_mmap err: {}", err);
        return -1;
    }

    0
}

pub fn sys_munmap(start: usize, len: usize) -> isize {
    let len = ((len - 1) / 4096 + 1) * 4096;

    let start_va: VirtAddr = start.into();
    if start_va.page_offset() != 0 {
        return -1;
    }

    let end_va: VirtAddr = (start + len - 1).into();

    if let Err(err) = memeory_unmap(start_va, end_va) {
        error!("sys_mmap err: {}", err);
        return -1;
    }

    0
}

//
// YOUR JOB: ?????? sys_spawn ????????????
// ALERT: ??????????????? SPAWN ??????????????????????????????????????????SPAWN != FORK + EXEC
pub fn sys_spawn(path: *const u8) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let current_task = current_task().unwrap();
        let new_task = current_task.spawn(data);
        let new_pid = new_task.pid.0;
        // add new task to scheduler
        add_task(new_task);
        new_pid as isize
    } else {
        error!("no such file!");
        -1
    }
}
