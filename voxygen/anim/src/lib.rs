#![feature(const_generics)]
#![feature(or_patterns)]
#![allow(incomplete_features)]
#[cfg(all(feature = "be-dyn-lib", feature = "use-dyn-lib"))]
compile_error!("Can't use both \"be-dyn-lib\" and \"use-dyn-lib\" features at once");

macro_rules! skeleton_impls {
    { struct $Skeleton:ident { $( $(+)? $bone:ident ),* $(,)? } } => {
        #[derive(Clone, Default)]
        pub struct $Skeleton {
            $(
                $bone: $crate::Bone,
            )*
        }

        impl<'a, Factor> $crate::vek::Lerp<Factor> for &'a $Skeleton
            where
                Factor: Copy,
                $crate::Bone: Lerp<Factor, Output=$crate::Bone>
        {
            type Output = $Skeleton;

            fn lerp_unclamped_precise(from: Self, to: Self, factor: Factor) -> Self::Output {
                Self::Output {
                    $(
                        $bone: Lerp::lerp_unclamped_precise(from.$bone, to.$bone, factor),
                    )*
                }
            }

            fn lerp_unclamped(from: Self, to: Self, factor: Factor) -> Self::Output {
                Self::Output {
                    $(
                        $bone: Lerp::lerp_unclamped(from.$bone, to.$bone, factor),
                    )*
                }
            }
        }
    }
}

pub mod biped_large;
pub mod biped_small;
pub mod bird_medium;
pub mod bird_small;
pub mod character;
pub mod dragon;
#[cfg(feature = "use-dyn-lib")] pub mod dyn_lib;
pub mod fish_medium;
pub mod fish_small;
pub mod fixture;
pub mod golem;
pub mod object;
pub mod ship;
pub mod quadruped_low;
pub mod quadruped_medium;
pub mod quadruped_small;
pub mod theropod;
pub mod vek;

#[cfg(feature = "use-dyn-lib")]
pub use dyn_lib::init;

#[cfg(feature = "use-dyn-lib")]
use std::ffi::CStr;

use self::vek::*;

type MatRaw = [[f32; 4]; 4];

pub type FigureBoneData = (MatRaw, MatRaw);

pub const MAX_BONE_COUNT: usize = 16;

fn make_bone(mat: Mat4<f32>) -> FigureBoneData {
    let normal = mat.map_cols(Vec4::normalized);
    (mat.into_col_arrays(), normal.into_col_arrays())
}

pub type Bone = Transform<f32, f32, f32>;

pub trait Skeleton: Send + Sync + 'static {
    type Attr;
    type Body;

    const BONE_COUNT: usize;

    #[cfg(feature = "use-dyn-lib")]
    const COMPUTE_FN: &'static [u8];

    fn compute_matrices_inner(
        &self,
        base_mat: Mat4<f32>,
        buf: &mut [FigureBoneData; MAX_BONE_COUNT],
    ) -> Vec3<f32>;
}

pub fn compute_matrices<S: Skeleton>(
    skeleton: &S,
    base_mat: Mat4<f32>,
    buf: &mut [FigureBoneData; MAX_BONE_COUNT],
) -> Vec3<f32> {
    #[cfg(not(feature = "use-dyn-lib"))]
    {
        S::compute_matrices_inner(skeleton, base_mat, buf)
    }
    #[cfg(feature = "use-dyn-lib")]
    {
        let lock = dyn_lib::LIB.lock().unwrap();
        let lib = &lock.as_ref().unwrap().lib;

        let compute_fn: libloading::Symbol<
            fn(&S, Mat4<f32>, &mut [FigureBoneData; MAX_BONE_COUNT]) -> Vec3<f32>,
        > = unsafe { lib.get(S::COMPUTE_FN) }.unwrap_or_else(|e| {
            panic!(
                "Trying to use: {} but had error: {:?}",
                CStr::from_bytes_with_nul(S::COMPUTE_FN)
                    .map(CStr::to_str)
                    .unwrap()
                    .unwrap(),
                e
            )
        });

        compute_fn(skeleton, base_mat, buf)
    }
}

pub trait Animation {
    type Skeleton: Skeleton;
    type Dependency;

    #[cfg(feature = "use-dyn-lib")]
    const UPDATE_FN: &'static [u8];

    /// Returns a new skeleton that is generated by the animation.
    fn update_skeleton_inner(
        _skeleton: &Self::Skeleton,
        _dependency: Self::Dependency,
        _anim_time: f32,
        _rate: &mut f32,
        _skeleton_attr: &<<Self as Animation>::Skeleton as Skeleton>::Attr,
    ) -> Self::Skeleton;

    /// Calls `update_skeleton_inner` either directly or via `libloading` to
    /// generate the new skeleton.
    fn update_skeleton(
        skeleton: &Self::Skeleton,
        dependency: Self::Dependency,
        anim_time: f32,
        rate: &mut f32,
        skeleton_attr: &<<Self as Animation>::Skeleton as Skeleton>::Attr,
    ) -> Self::Skeleton {
        #[cfg(not(feature = "use-dyn-lib"))]
        {
            Self::update_skeleton_inner(skeleton, dependency, anim_time, rate, skeleton_attr)
        }
        #[cfg(feature = "use-dyn-lib")]
        {
            let lock = dyn_lib::LIB.lock().unwrap();
            let lib = &lock.as_ref().unwrap().lib;

            let update_fn: libloading::Symbol<
                fn(
                    &Self::Skeleton,
                    Self::Dependency,
                    f32,
                    &mut f32,
                    &<Self::Skeleton as Skeleton>::Attr,
                ) -> Self::Skeleton,
            > = unsafe {
                //let start = std::time::Instant::now();
                // Overhead of 0.5-5 us (could use hashmap to mitigate if this is an issue)
                let f = lib.get(Self::UPDATE_FN);
                //println!("{}", start.elapsed().as_nanos());
                f
            }
            .unwrap_or_else(|e| {
                panic!(
                    "Trying to use: {} but had error: {:?}",
                    CStr::from_bytes_with_nul(Self::UPDATE_FN)
                        .map(CStr::to_str)
                        .unwrap()
                        .unwrap(),
                    e
                )
            });

            update_fn(skeleton, dependency, anim_time, rate, skeleton_attr)
        }
    }
}
