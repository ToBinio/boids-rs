use std::ops::Sub;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use spatial_neighbors::quad_tree::QuadTree;
use spatial_neighbors::SpatialPartitioner;
use wgpu::include_wgsl;
use wgpu::util::{DeviceExt, StagingBelt};
use wgpu_glyph::{ab_glyph, GlyphBrush, GlyphBrushBuilder, Section, Text};
use winit::event::WindowEvent;
use winit::window::Window;

use crate::boid::Boid;
use crate::vec2::Vec2;
use crate::vertex::Vertex;

pub struct State {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,

    render_pipeline: wgpu::RenderPipeline,

    boids: Vec<Boid>,

    staging_belt: StagingBelt,
    glyph_brush: GlyphBrush<()>,

    update_time: (u128, u128),
    render_time: u128,
}

impl State {
    // Creating some of the wgpu types requires async code
    pub async fn new(window: &Window) -> Self {
        let size = window.inner_size();

        // The instance is a handle to our GPU
        // Backends::all => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = wgpu::Instance::new(wgpu::Backends::all());

        let surface = unsafe { instance.create_surface(window) };

        let adapter = instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            },
        ).await.unwrap();

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                features: wgpu::Features::empty(),
                // WebGL doesn't support all of wgpu's features, so if
                // we're building for the web we'll have to disable some.
                limits: if cfg!(target_arch = "wasm32") {
                    wgpu::Limits::downlevel_webgl2_defaults()
                } else {
                    wgpu::Limits::default()
                },
                label: None,
            },
            None, // Trace path
        ).await.unwrap();

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(include_wgsl!("shader.wgsl"));

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main", // 1.
                buffers: &[Vertex::desc()], // 2.
            },
            fragment: Some(wgpu::FragmentState { // 3.
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState { // 4.
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList, // 1.
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw, // 2.
                cull_mode: Some(wgpu::Face::Back),
                // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
                polygon_mode: wgpu::PolygonMode::Fill,
                // Requires Features::DEPTH_CLIP_CONTROL
                unclipped_depth: false,
                // Requires Features::CONSERVATIVE_RASTERIZATION
                conservative: false,
            },
            depth_stencil: None, // 1.
            multisample: wgpu::MultisampleState {
                count: 1, // 2.
                mask: !0, // 3.
                alpha_to_coverage_enabled: false, // 4.
            },
            multiview: None, // 5.
        });

        // Create staging belt
        let staging_belt = StagingBelt::new(1024);

        // Prepare glyph_brush
        let inconsolata = ab_glyph::FontArc::try_from_slice(include_bytes!(
            "Inconsolata-Regular.ttf"
        )).unwrap();

        let glyph_brush = GlyphBrushBuilder::using_font(inconsolata)
            .build(&device, wgpu::TextureFormat::Bgra8UnormSrgb);

        let mut boids = Vec::new();

        for _ in 0..20000 {
            boids.push(Boid::new_random());
        }

        Self {
            surface,
            device,
            queue,
            config,
            size,
            render_pipeline,

            boids,

            staging_belt,
            glyph_brush,

            update_time: (0, 0),
            render_time: 0,
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn input(&mut self, _event: &WindowEvent) -> bool {
        false
    }

    pub fn update(&mut self) {
        let start_time = Instant::now();

        const RADIUS: f64 = 0.03;

        let mut quad_tree = QuadTree::with_capacity(-1.1..1.1, -1.1..1.1, 75);

        for (index, boid) in self.boids.iter().enumerate() {
            quad_tree.insert((boid.location.x as f64, boid.location.y as f64), index);
        }

        let quad_tree = Arc::new(quad_tree);
        let boids = Arc::new(self.boids.clone());

        let thread_count = num_cpus::get();

        let mut threads = Vec::new();

        let boid_count = self.boids.len();
        let boids_per_thread = boid_count as f32 / thread_count as f32;

        for i in 0..thread_count {
            let range = (boids_per_thread * i as f32).ceil() as usize..((boids_per_thread * (i + 1) as f32).ceil() as usize);

            let boids = boids.clone();
            let quad_tree = quad_tree.clone();

            threads.push(thread::spawn(move || {
                let mut new_vel = Vec::with_capacity(boids.len());

                for index in range {
                    let boid = boids.get(index).unwrap();
                    let neighbor_boids = quad_tree.in_circle((boid.location.x as f64, boid.location.y as f64), RADIUS);

                    let mut separation = Vec2::new(0.0, 0.0);
                    let mut alignment = Vec2::new(0.0, 0.0);
                    let mut cohesion = Vec2::new(0.0, 0.0);

                    for neighbor_boid in &neighbor_boids {
                        if index == *neighbor_boid {
                            continue;
                        }

                        let neighbor_boid = boids.get(*neighbor_boid).unwrap();

                        let mut separation_vec = boid.location.clone();
                        separation_vec.sub(&neighbor_boid.location);

                        let new_length = ((RADIUS as f32 - separation_vec.length()) / RADIUS as f32).powi(3);

                        separation_vec.normalize();
                        separation_vec.mul(new_length);

                        separation.add(&separation_vec);
                        alignment.add(&neighbor_boid.vel);

                        cohesion.add(&neighbor_boid.location);
                    }

                    separation.div(neighbor_boids.len() as f32);
                    separation.mul(2.0);

                    alignment.div(neighbor_boids.len() as f32);
                    alignment.mul(0.5);

                    cohesion.div(neighbor_boids.len() as f32);
                    cohesion.sub(&boid.location);
                    cohesion.mul(0.6);

                    cohesion.add(&separation);
                    cohesion.add(&alignment);

                    new_vel.push(cohesion);
                }

                new_vel
            }));
        }

        let mut index = 0;

        let mut new_vels = Vec::new();

        for thread in threads {
            new_vels.push(thread.join().expect("TODO: panic message"));
        }

        self.update_time.0 = (start_time.elapsed().as_nanos() + self.update_time.0 * 59) / 60;
        let start_time = Instant::now();

        for mut vec in new_vels {
            for boid_vel in &mut vec {
                let boid = self.boids.get_mut(index).unwrap();

                boid.add_vel(boid_vel, 0.6);
                boid.update();

                index += 1;
            }
        }

        self.update_time.1 = (start_time.elapsed().as_nanos() + self.update_time.1 * 59) / 60;
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let start_time = Instant::now();

        let frame = self.surface.get_current_texture()?;

        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        let mut vertices = Vec::new();
        let mut indices: Vec<u32> = Vec::new();

        for (index, boid) in self.boids.iter().enumerate() {
            boid.create_buffer(&mut vertices, &mut indices, index as u32);
        }

        let vertex_buffer = self.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }
        );

        let index_buffer = self.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Index Buffer"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            }
        );

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.1,
                        g: 0.2,
                        b: 0.3,
                        a: 1.0,
                    }),
                    store: true,
                },
            })],
            depth_stencil_attachment: None,
        });

        render_pass.set_pipeline(&self.render_pipeline); // 2.

        render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);

        render_pass.draw_indexed(0..indices.len() as u32, 0, 0..1); // 3.

        drop(render_pass);

        let render_time = self.render_time as f64 / 1_000_000.0;
        let update_time = (self.update_time.0 as f64 / 1_000_000.0, self.update_time.1 as f64 / 1_000_000.0);
        let sum = render_time + update_time.1 + update_time.0;
        let fps = 1000.0 / sum;

        self.glyph_brush.queue(Section {
            screen_position: (10.0, 10.0),
            bounds: (self.size.width as f32, self.size.height as f32),
            text: vec![Text::new(format!("render: {:.1}ms\nupdate: {:.1}/{:.1}ms\nsum: {:.1}ms\nmax fps: {:.1}", render_time, update_time.0, update_time.1, sum, fps).as_str())
                .with_color([0.0, 0.0, 0.0, 1.0])
                .with_scale(20.0)],
            ..Section::default()
        });

        // Draw the text!
        self.glyph_brush.draw_queued(
            &self.device,
            &mut self.staging_belt,
            &mut encoder,
            &view,
            self.size.width,
            self.size.height,
        )
            .expect("Draw queued");


        self.staging_belt.finish();
        // submit will accept anything that implements IntoIter
        self.queue.submit([encoder.finish()]);

        self.render_time = (start_time.elapsed().as_nanos() + self.render_time * 59) / 60;

        frame.present();

        self.staging_belt.recall();


        Ok(())
    }
}