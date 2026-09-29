use alloc::vec::Vec;
use ash::vk;
use core::mem::MaybeUninit;

#[derive(Debug, PartialEq)]
pub enum SemaphoreListMode {
    Wait,
    Signal,
}

#[derive(Debug)]
pub struct SemaphoreList {

    mode: SemaphoreListMode,

    semaphores: Vec<vk::Semaphore>,

    values: Vec<u64>,

    pub stage_masks: Vec<vk::PipelineStageFlags>,
}

impl SemaphoreList {
    pub fn new(mode: SemaphoreListMode) -> Self {
        Self {
            mode,
            semaphores: Vec::new(),
            values: Vec::new(),
            stage_masks: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.semaphores.is_empty()
    }

    pub fn add_to_submit<'info, 'semaphores: 'info>(
        wait_semaphores: &'semaphores mut Self,
        signal_semaphores: &'semaphores mut Self,
        submit_info: vk::SubmitInfo<'info>,
        timeline_info: &'info mut MaybeUninit<vk::TimelineSemaphoreSubmitInfo<'info>>,
    ) -> vk::SubmitInfo<'info> {
        wait_semaphores.check();
        signal_semaphores.check();

        assert!(matches!(wait_semaphores.mode, SemaphoreListMode::Wait));
        assert!(matches!(signal_semaphores.mode, SemaphoreListMode::Signal));

        let timeline_info = timeline_info.write(vk::TimelineSemaphoreSubmitInfo::default());

        let mut uses_timeline = false;

        if !wait_semaphores.values.is_empty() {
            *timeline_info = timeline_info.wait_semaphore_values(&wait_semaphores.values);
            uses_timeline = true;
        }

        if !signal_semaphores.values.is_empty() {
            *timeline_info = timeline_info.signal_semaphore_values(&signal_semaphores.values);
            uses_timeline = true;
        }

        let mut submit_info = submit_info
            .wait_semaphores(&wait_semaphores.semaphores)
            .wait_dst_stage_mask(&wait_semaphores.stage_masks)
            .signal_semaphores(&signal_semaphores.semaphores);

        if uses_timeline {
            submit_info = submit_info.push_next(timeline_info);
        }

        submit_info
    }

    pub fn push_signal(&mut self, semaphore: SemaphoreType) {
        assert!(matches!(self.mode, SemaphoreListMode::Signal));
        self.push_inner(semaphore);
    }

    pub fn push_wait(&mut self, semaphore: SemaphoreType, stage: vk::PipelineStageFlags) {
        assert!(matches!(self.mode, SemaphoreListMode::Wait));

        self.stage_masks.push(stage);
        self.push_inner(semaphore);
    }

    fn push_inner(&mut self, semaphore: SemaphoreType) {
        match semaphore {
            SemaphoreType::Binary(semaphore) => {
                self.semaphores.push(semaphore);

                if !self.values.is_empty() {
                    self.values.push(!0);
                }
            }
            SemaphoreType::Timeline(semaphore, value) => {

                self.pad_values();
                self.semaphores.push(semaphore);
                self.values.push(value);
            }
        }

        self.check();
    }

    pub fn remove(&mut self, semaphore: vk::Semaphore) -> bool {
        let mut removed = false;
        let mut i = 0;
        while i < self.semaphores.len() {
            if self.semaphores[i] == semaphore {
                self.semaphores.swap_remove(i);
                if !self.values.is_empty() {
                    self.values.swap_remove(i);
                }
                if !self.stage_masks.is_empty() {
                    self.stage_masks.swap_remove(i);
                }
                removed = true;
            } else {
                i += 1;
            }
        }
        self.check();
        removed
    }

    pub fn append(&mut self, other: &mut Self) {
        assert_eq!(self.mode, other.mode);

        if !other.values.is_empty() {
            self.pad_values();
        }
        self.semaphores.append(&mut other.semaphores);
        self.values.append(&mut other.values);

        if !self.values.is_empty() {
            self.pad_values();
        }
        self.stage_masks.append(&mut other.stage_masks);
        self.check();
    }

    fn pad_values(&mut self) {
        self.values.resize(self.semaphores.len(), !0);
    }

    #[track_caller]
    fn check(&self) {
        debug_assert!(self.values.is_empty() || self.values.len() == self.semaphores.len());
        match self.mode {
            SemaphoreListMode::Wait => {
                debug_assert!(
                    self.stage_masks.is_empty() || self.stage_masks.len() == self.semaphores.len()
                );
            }
            SemaphoreListMode::Signal => {
                debug_assert!(self.stage_masks.is_empty());
            }
        }
    }
}

pub enum SemaphoreType {
    Binary(vk::Semaphore),
    Timeline(vk::Semaphore, u64),
}
