use std::sync::Arc;

use cros_codecs::backend::vaapi::encoder::tests::TestFrameGenerator;
use cros_codecs::backend::vaapi::encoder::VaapiBackend;
use cros_codecs::backend::vaapi::surface_pool::{PooledVaSurface, VaSurfacePool};
use cros_codecs::codec::h264::parser::Profile;
use cros_codecs::decoder::FramePool;
use cros_codecs::encoder::h264::EncoderConfig;
use cros_codecs::encoder::stateless::h264::StatelessEncoder;
use cros_codecs::encoder::{simple_encode_loop, RateControl, Tunings};
use cros_codecs::{BlockingMode, FrameLayout, PlaneLayout, Resolution};
use libva::UsageHint;
use libva::VAEntrypoint::VAEntrypointEncSliceLP;
use libva::VAProfile::VAProfileH264Main;
use libva::VA_RT_FORMAT_YUV420;
use std::io::Write;

fn main() {
    type VaapiH264Encoder<'l> =
        StatelessEncoder<PooledVaSurface<()>, VaapiBackend<(), PooledVaSurface<()>>>;

    const WIDTH: usize = 512;
    const HEIGHT: usize = 512;

    let _ = env_logger::try_init();

    let display = libva::Display::open_drm_display("/dev/dri/renderD129").unwrap();
    let entrypoints = display.query_config_entrypoints(VAProfileH264Main).unwrap();
    let low_power = entrypoints.contains(&VAEntrypointEncSliceLP);

    let config = EncoderConfig {
        profile: Profile::Main,
        resolution: Resolution { width: WIDTH as u32, height: HEIGHT as u32 },
        initial_tunings: Tunings {
            rate_control: RateControl::ConstantBitrate(1_200_000),
            framerate: 30,
            ..Default::default()
        },
        ..Default::default()
    };

    let frame_layout = FrameLayout {
        format: (b"NV12".into(), 0),
        size: Resolution { width: WIDTH as u32, height: HEIGHT as u32 },
        planes: vec![
            PlaneLayout { buffer_index: 0, offset: 0, stride: WIDTH },
            PlaneLayout { buffer_index: 0, offset: WIDTH * HEIGHT, stride: WIDTH },
        ],
    };

    let mut encoder = VaapiH264Encoder::new_native_vaapi(
        Arc::clone(&display),
        config,
        frame_layout.format.0,
        frame_layout.size,
        low_power,
        BlockingMode::Blocking,
    )
    .unwrap();

    let mut pool = VaSurfacePool::new(
        Arc::clone(&display),
        VA_RT_FORMAT_YUV420,
        Some(UsageHint::USAGE_HINT_ENCODER),
        Resolution { width: WIDTH as u32, height: HEIGHT as u32 },
    );

    pool.add_frames(vec![(); 16]).unwrap();

    let mut frame_producer = TestFrameGenerator::new(150, display, pool, frame_layout);

    let mut bitstream = Vec::new();

    simple_encode_loop(&mut encoder, &mut frame_producer, |coded| {
        bitstream.extend(coded.bitstream)
    })
    .unwrap();

    let mut out = std::fs::File::create("test_vaapi_encoder.h264").unwrap();
    out.write_all(&bitstream).unwrap();
    out.flush().unwrap();
}
