use gstreamer::{
    glib::{
        subclass::{object::ObjectImplExt, types::ObjectSubclassExt},
        HasParamSpec, ParamSpecInt,
    },
    DebugCategory, Object, StateChangeError,
};
use rayon::{ThreadPool, ThreadPoolBuildError, ThreadPoolBuilder};
use std::{
    io,
    sync::{
        atomic::{AtomicI32, AtomicU32, Ordering::Relaxed},
        nonpoison::RwLock,
        LazyLock,
    },
};

enum ThreadPoolKind {
    Nothing,
    UseGlobalPool,
    SomePool(ThreadPool),
}

pub struct SimpleThreadPool {
    threadpool: RwLock<ThreadPoolKind>,
}

impl Default for SimpleThreadPool {
    fn default() -> Self {
        Self {
            threadpool: RwLock::new(ThreadPoolKind::Nothing),
        }
    }
}

impl SimpleThreadPool {
    pub fn install<R: Send>(&self, f: impl FnOnce() -> R + Send) -> R {
        let pool = self.threadpool.read();
        match &*pool {
            ThreadPoolKind::Nothing => {
                panic!("ThreadPool not initalized yet! This is probably a state-change bug")
            }
            ThreadPoolKind::UseGlobalPool => f(),
            ThreadPoolKind::SomePool(thread_pool) => thread_pool.install(f),
        }
    }

    pub fn reconfigure(
        &self,
        nthreads: &AtomicI32,
        obj: &impl ObjectImplExt,
        cat: &LazyLock<DebugCategory>,
    ) -> Result<(), StateChangeError> {
        let mut nthreads = nthreads.load(Relaxed);
        if nthreads == -1 {
            *self.threadpool.write() = ThreadPoolKind::UseGlobalPool;
            return Ok(());
        } else if nthreads < -1 {
            gstreamer::error!(*cat, imp = obj, "nthreads must be -1, 0, or <nthreads>");
            return Err(StateChangeError);
        } else if nthreads == 0 {
            nthreads = num_cpus::get() as i32;
        }

        let new_pool = ThreadPoolBuilder::new()
            .num_threads(nthreads as usize)
            .build()
            .map_err(|e| {
                gstreamer::error!(
                    *cat,
                    imp = obj,
                    "Failed to create threadpool of size `{nthreads}`: {e}"
                );
                StateChangeError
            })?;
        *self.threadpool.write() = ThreadPoolKind::SomePool(new_pool);
        Ok(())
    }
}
