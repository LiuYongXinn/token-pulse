#![cfg(windows)]
use token_pulse_taskbar::windows::topology::{ProbeError, ScreenRect, TaskbarTopology};
fn topology(dpi: u32) -> TaskbarTopology {
    let scale = |v: i32| v * dpi as i32 / 96;
    let rect = |left, right| ScreenRect {
        left: scale(left),
        top: -scale(48),
        right: scale(right),
        bottom: 0,
    };
    TaskbarTopology {
        build: 19045,
        dpi,
        taskbar: rect(-1920, 0),
        rebar: rect(-1500, -300),
        task_switch: rect(-1498, -300),
        task_list: rect(-1498, -300),
        notification: rect(-300, 0),
    }
}
#[test]
fn reservation_uses_measured_pixels_and_preserves_disjoint_system_regions_at_four_dpis() {
    for dpi in [96, 120, 144, 192] {
        let topology = topology(dpi);
        let width = (360 * dpi / 96) as i32;
        let plan = topology.plan(width, (320 * dpi / 96) as i32).unwrap();
        assert_eq!(plan.host.width(), width);
        assert_eq!(plan.host.right, topology.task_switch.right);
        assert_eq!(plan.remaining_task_switch.left, topology.task_switch.left);
        assert_eq!(plan.remaining_task_switch.right, plan.host.left);
        assert!(plan.host.right <= topology.notification.left);
        assert_eq!(plan.host.top, topology.rebar.top);
        assert_eq!(plan.host.bottom, topology.rebar.bottom);
    }
}
#[test]
fn unknown_build_vertical_invalid_dpi_overlap_and_extra_gap_do_not_get_a_plan() {
    for kind in 0..7 {
        let mut topology = topology(96);
        match kind {
            0 => topology.build = 26100,
            1 => topology.dpi = 0,
            2 => topology.taskbar.right = topology.taskbar.left + 20,
            3 => topology.notification.left -= 1,
            4 => topology.task_switch.right -= 1,
            5 => topology.task_list.left -= 10,
            _ => topology.task_switch.left = i32::MIN,
        }
        assert!(topology.plan(360, 320).is_err());
    }
}
#[test]
fn occupied_width_cannot_be_reserved_or_overflowed() {
    let topology = topology(96);
    assert_eq!(topology.plan(1000, 320), Err(ProbeError::InsufficientSpace));
    assert_eq!(
        topology.plan(i32::MAX, i32::MAX),
        Err(ProbeError::InsufficientSpace)
    );
    assert_eq!(topology.plan(0, 320), Err(ProbeError::UnsafeGeometry));
    let full = topology.plan(360, 320).unwrap();
    let compact = topology.plan(198, 320).unwrap();
    assert!(compact.remaining_task_switch.width() > full.remaining_task_switch.width());
    assert_eq!(compact.host.width(), 198);
}
