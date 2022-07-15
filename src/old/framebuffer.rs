pub struct Framebuffer {
    pub sample_count: u32,
    pub color_target: wgpu::TextureView,
    pub normal_view: wgpu::TextureView,
    pub depth_view: wgpu::TextureView,
}

impl Framebuffer {
    pub const NORMAL: wgpu::TextureFormat = wgpu::TextureFormat::Rgb10a2Unorm;
    pub const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    pub fn new(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        sample_count: u32,
    ) -> Self {
        let color_target = Self::multisampled(
            device,
            config,
            sample_count,
            config.format,
            wgpu::TextureUsages::empty(),
        );
        let normal_view = Self::multisampled(
            device,
            config,
            sample_count,
            Self::NORMAL,
            wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let depth_view = Self::depth(device, config, sample_count);

        Self {
            sample_count,
            color_target,
            normal_view,
            depth_view,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) {
        self.color_target = Self::multisampled(
            device,
            config,
            self.sample_count,
            config.format,
            wgpu::TextureUsages::empty(),
        );
        self.normal_view = Self::multisampled(
            device,
            config,
            self.sample_count,
            Self::NORMAL,
            wgpu::TextureUsages::TEXTURE_BINDING,
        );
        self.depth_view = Self::depth(device, config, self.sample_count);
    }

    pub fn target<'a>(
        &'a self,
        view: &'a wgpu::TextureView,
    ) -> (&'a wgpu::TextureView, Option<&'a wgpu::TextureView>) {
        if self.sample_count <= 1 {
            (view, None)
        } else {
            (&self.color_target, Some(view))
        }
    }

    fn multisampled(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        sample_count: u32,
        format: wgpu::TextureFormat,
        usages: wgpu::TextureUsages,
    ) -> wgpu::TextureView {
        let size = wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        };
        let multisampled_frame_descriptor = &wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | usages,
        };

        device
            .create_texture(multisampled_frame_descriptor)
            .create_view(&wgpu::TextureViewDescriptor::default())
    }

    fn depth(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        sample_count: u32,
    ) -> wgpu::TextureView {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        });

        texture.create_view(&wgpu::TextureViewDescriptor::default())
    }
}
