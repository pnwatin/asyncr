use std::task::Poll;

pub(super) enum ScheduleState {
    Idle,
    Queued,
    Running { notified: bool },
    Complete,
}

impl ScheduleState {
    pub(super) fn wake(&mut self) -> ScheduleAction {
        match self {
            Self::Idle => {
                *self = Self::Queued;
                ScheduleAction::Enqueue
            }
            Self::Running { notified: false } => {
                *self = Self::Running { notified: true };
                ScheduleAction::None
            }
            _ => ScheduleAction::None,
        }
    }

    pub(super) fn begin_poll(&mut self) {
        match self {
            Self::Queued => *self = Self::Running { notified: false },
            _ => unreachable!("only Queued tasks should be polled"),
        }
    }

    pub(super) fn finish_poll(&mut self, result: Poll<()>) -> ScheduleAction {
        match self {
            Self::Running { notified: _ } if result.is_ready() => {
                *self = Self::Complete;
                ScheduleAction::Complete
            }
            Self::Running { notified: false } if result.is_pending() => {
                *self = Self::Idle;
                ScheduleAction::None
            }
            Self::Running { notified: true } if result.is_pending() => {
                *self = Self::Queued;
                ScheduleAction::Enqueue
            }
            _ => {
                unreachable!("task must be running when poll finishes");
            }
        }
    }
}

pub(super) enum ScheduleAction {
    None,
    Enqueue,
    Complete,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waking_idle_task_queues_it() {
        let mut state = ScheduleState::Idle;

        let action = state.wake();

        assert!(matches!(state, ScheduleState::Queued));
        assert!(matches!(action, ScheduleAction::Enqueue));
    }

    #[test]
    fn waking_queued_task_is_a_noop() {
        let mut state = ScheduleState::Queued;

        let action = state.wake();

        assert!(matches!(state, ScheduleState::Queued));
        assert!(matches!(action, ScheduleAction::None));
    }

    #[test]
    fn waking_running_task_marks_it_notified() {
        let mut state = ScheduleState::Running { notified: false };

        let action = state.wake();

        assert!(matches!(state, ScheduleState::Running { notified: true }));
        assert!(matches!(action, ScheduleAction::None));
    }

    #[test]
    fn repeated_wake_while_running_is_coalesced() {
        let mut state = ScheduleState::Running { notified: true };

        let action = state.wake();

        assert!(matches!(state, ScheduleState::Running { notified: true },));
        assert!(matches!(action, ScheduleAction::None));
    }

    #[test]
    fn polling_queued_task_moves_it_to_running() {
        let mut state = ScheduleState::Queued;

        state.begin_poll();

        assert!(matches!(state, ScheduleState::Running { notified: false }));
    }

    #[test]
    fn pending_unnotified_task_becomes_idle() {
        let mut state = ScheduleState::Running { notified: false };

        let action = state.finish_poll(Poll::Pending);

        assert!(matches!(state, ScheduleState::Idle));
        assert!(matches!(action, ScheduleAction::None));
    }

    #[test]
    fn pending_notified_task_is_requeued() {
        let mut state = ScheduleState::Running { notified: true };

        let action = state.finish_poll(Poll::Pending);

        assert!(matches!(state, ScheduleState::Queued));
        assert!(matches!(action, ScheduleAction::Enqueue));
    }

    #[test]
    fn ready_unnotified_task_becomes_complete() {
        let mut state = ScheduleState::Running { notified: false };

        let action = state.finish_poll(Poll::Ready(()));

        assert!(matches!(state, ScheduleState::Complete));
        assert!(matches!(action, ScheduleAction::Complete));
    }

    #[test]
    fn ready_notified_task_becomes_complete() {
        let mut state = ScheduleState::Running { notified: true };

        let action = state.finish_poll(Poll::Ready(()));

        assert!(matches!(state, ScheduleState::Complete));
        assert!(matches!(action, ScheduleAction::Complete));
    }
}
