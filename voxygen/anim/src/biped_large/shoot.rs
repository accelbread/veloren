use super::{
    super::{vek::*, Animation},
    BipedLargeSkeleton, SkeletonAttr,
};
use common::{comp::item::ToolKind, states::utils::StageSection};
use std::f32::consts::PI;

pub struct ShootAnimation;

type ShootAnimationDependency = (
    Option<ToolKind>,
    Option<ToolKind>,
    Vec3<f32>,
    Vec3<f32>,
    Vec3<f32>,
    f64,
    Option<StageSection>,
    f32,
);
impl Animation for ShootAnimation {
    type Dependency = ShootAnimationDependency;
    type Skeleton = BipedLargeSkeleton;

    #[cfg(feature = "use-dyn-lib")]
    const UPDATE_FN: &'static [u8] = b"biped_large_shoot\0";

    #[cfg_attr(feature = "be-dyn-lib", export_name = "biped_large_shoot")]
    fn update_skeleton_inner(
        skeleton: &Self::Skeleton,
        (
            active_tool_kind,
            _second_tool_kind,
            velocity,
            _orientation,
            _last_ori,
            _global_time,
            stage_section,
            acc_vel,
        ): Self::Dependency,
        anim_time: f64,
        rate: &mut f32,
        s_a: &SkeletonAttr,
    ) -> Self::Skeleton {
        *rate = 1.0;
        let speed = Vec2::<f32>::from(velocity).magnitude();

        let mut next = (*skeleton).clone();

        let lab = 0.65 * s_a.tempo;
        let speednorm = (speed / 12.0).powf(0.4);
        let foothoril = (acc_vel * lab as f32 + PI * 1.45).sin() * speednorm;
        let foothorir = (acc_vel * lab as f32 + PI * (0.45)).sin() * speednorm;
        let footrotl =
            (((1.0) / (0.5 + (0.5) * ((acc_vel * lab as f32 + PI * 1.4).sin()).powi(2))).sqrt())
                * ((acc_vel * lab as f32 + PI * 1.4).sin());

        let footrotr =
            (((1.0) / (0.5 + (0.5) * ((acc_vel * lab as f32 + PI * 0.4).sin()).powi(2))).sqrt())
                * ((acc_vel * lab as f32 + PI * 0.4).sin());

        next.shoulder_l.position = Vec3::new(
            -s_a.shoulder.0,
            s_a.shoulder.1,
            s_a.shoulder.2 - foothorir * 1.0,
        );
        next.shoulder_l.orientation =
            Quaternion::rotation_x(0.8 + 1.2 * speednorm + (footrotr * -0.2) * speednorm);

        next.shoulder_r.position = Vec3::new(
            s_a.shoulder.0,
            s_a.shoulder.1,
            s_a.shoulder.2 - foothoril * 1.0,
        );
        next.shoulder_r.orientation =
            Quaternion::rotation_x(0.8 + 1.2 * speednorm + (footrotl * -0.2) * speednorm);
        next.jaw.position = Vec3::new(0.0, s_a.jaw.0, s_a.jaw.1);
        next.jaw.orientation = Quaternion::rotation_x(0.0);

        next.main.position = Vec3::new(0.0, 0.0, 0.0);
        next.main.orientation = Quaternion::rotation_x(0.0);

        next.hand_l.position = Vec3::new(0.0, 0.0, s_a.grip);
        next.hand_r.position = Vec3::new(0.0, 0.0, s_a.grip);

        next.hand_l.orientation = Quaternion::rotation_x(0.0);
        next.hand_r.orientation = Quaternion::rotation_x(0.0);
        match active_tool_kind {
            Some(ToolKind::StaffSimple) | Some(ToolKind::Sceptre) => {
                let (movement1base, movement1shake, movement2base, movement3) = match stage_section
                {
                    Some(StageSection::Buildup) => (
                        anim_time as f32,
                        (anim_time as f32 * 10.0 + PI).sin(),
                        0.0,
                        0.0,
                    ),
                    Some(StageSection::Swing) => (1.0, 1.0, (anim_time as f32).powf(0.25), 0.0),
                    Some(StageSection::Recover) => (1.0, 1.0, 1.0, anim_time as f32),
                    _ => (0.0, 0.0, 0.0, 0.0),
                };
                let pullback = 1.0 - movement3;
                let movement1 = movement1base * pullback;
                let movement2 = movement2base * pullback;
                next.control_l.position = Vec3::new(-1.0, 3.0, 12.0);
                next.control_r.position = Vec3::new(1.0, 2.0, 2.0);

                next.control.position = Vec3::new(
                    -3.0,
                    3.0 + s_a.grip / 1.2
                        + movement1 * 4.0
                        + movement2
                        + movement1shake * 2.0
                        + movement2 * -2.0,
                    -11.0 + -s_a.grip / 2.0 + movement1 * 3.0,
                );
                next.head.orientation = Quaternion::rotation_x(movement1 * -0.15)
                    * Quaternion::rotation_y(movement1 * 0.25)
                    * Quaternion::rotation_z(movement1 * 0.25);
                next.jaw.orientation = Quaternion::rotation_x(movement1 * -0.5);

                next.control_l.orientation = Quaternion::rotation_x(PI / 2.0 + movement1 * 0.5)
                    * Quaternion::rotation_y(movement1 * -0.4);
                next.control_r.orientation = Quaternion::rotation_x(PI / 2.5 + movement1 * 0.5)
                    * Quaternion::rotation_y(0.5)
                    * Quaternion::rotation_z(0.0);

                next.control.orientation =
                    Quaternion::rotation_x(-0.2 + movement1 * -0.2 + movement1shake * 0.1)
                        * Quaternion::rotation_y(-0.1 + movement1 * 0.8 + movement2 * -0.3);
                next.shoulder_l.position = Vec3::new(
                    -s_a.shoulder.0,
                    s_a.shoulder.1,
                    s_a.shoulder.2 - foothorir * 1.0,
                );
                next.shoulder_l.orientation = Quaternion::rotation_x(
                    movement1 * 0.8 + 0.8 * speednorm + (footrotr * -0.2) * speednorm,
                );

                next.shoulder_r.position = Vec3::new(
                    s_a.shoulder.0,
                    s_a.shoulder.1,
                    s_a.shoulder.2 - foothoril * 1.0,
                );
                next.shoulder_r.orientation = Quaternion::rotation_x(
                    movement1 * 0.8 + 0.6 * speednorm + (footrotl * -0.2) * speednorm,
                );
            },
            Some(ToolKind::BowSimple) => {
                let (movement1base, movement2base, movement3) = match stage_section {
                    Some(StageSection::Buildup) => ((anim_time as f32).powf(0.25), 0.0, 0.0),
                    Some(StageSection::Swing) => (1.0, anim_time as f32, 0.0),
                    Some(StageSection::Recover) => (1.0, 1.0, (anim_time as f32).powi(4)),
                    _ => (0.0, 0.0, 0.0),
                };
                let pullback = 1.0 - movement3;
                let movement1 = movement1base * pullback;
                let movement2 = movement2base * pullback;
                next.control_l.position = Vec3::new(-1.0, -2.0 + movement2 * -7.0, -3.0);
                next.control_r.position = Vec3::new(0.0, 4.0, 1.0);

                next.control.position = Vec3::new(
                    -1.0 + movement1 * 2.0,
                    6.0 + s_a.grip / 1.2 + movement1 * 7.0,
                    -5.0 + -s_a.grip / 2.0 + movement1 * 8.0,
                );

                next.control_l.orientation = Quaternion::rotation_x(PI / 2.0 + movement2 * 0.4)
                    * Quaternion::rotation_y(-0.2);
                next.control_r.orientation = Quaternion::rotation_x(PI / 2.2 + movement1 * 0.4)
                    * Quaternion::rotation_y(0.2)
                    * Quaternion::rotation_z(0.0);

                next.control.orientation = Quaternion::rotation_x(-0.2)
                    * Quaternion::rotation_y(1.0 + movement1 * -0.4)
                    * Quaternion::rotation_z(-0.3);
                next.head.orientation = Quaternion::rotation_z(movement1 * 0.25);
                next.shoulder_l.position = Vec3::new(
                    -s_a.shoulder.0,
                    s_a.shoulder.1,
                    s_a.shoulder.2 - foothorir * 1.0,
                );
                next.shoulder_l.orientation = Quaternion::rotation_x(
                    movement1 * 0.8 + 1.2 * speednorm + (footrotr * -0.2) * speednorm,
                );

                next.shoulder_r.position = Vec3::new(
                    s_a.shoulder.0,
                    s_a.shoulder.1,
                    s_a.shoulder.2 - foothoril * 1.0,
                );
                next.shoulder_r.orientation = Quaternion::rotation_x(
                    movement1 * 0.8 + 1.2 * speednorm + (footrotl * -0.2) * speednorm,
                );
            },
            _ => {},
        }

        next
    }
}
