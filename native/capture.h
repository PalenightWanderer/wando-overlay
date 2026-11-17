// Windows.Graphics.Capture (WGC) sources. Each open slot captures one monitor
// or one window into a GPU texture; frames are never read back to the CPU.
//
// Capture uses its own D3D11 device: Ultralight creates its device with
// D3D11_CREATE_DEVICE_SINGLETHREADED, and the free-threaded WGC frame pool
// touches its device from a system thread.
namespace wgc = winrt::Windows::Graphics::Capture;
namespace wdx = winrt::Windows::Graphics::DirectX;

struct CaptureDevice {
  ComPtr<ID3D11Device> device;
  ComPtr<ID3D11DeviceContext> context;
  wdx::Direct3D11::IDirect3DDevice winrt{nullptr};
  CaptureDevice() {
    check(D3D11CreateDevice(nullptr,D3D_DRIVER_TYPE_HARDWARE,nullptr,D3D11_CREATE_DEVICE_BGRA_SUPPORT,nullptr,0,D3D11_SDK_VERSION,&device,nullptr,&context),"Capture device");
    ComPtr<IDXGIDevice> dxgi;check(device.As(&dxgi),"Capture DXGI device");
    winrt::com_ptr<::IInspectable> inspectable;
    check(CreateDirect3D11DeviceFromDXGIDevice(dxgi.Get(),inspectable.put()),"WinRT capture device");
    winrt=inspectable.as<wdx::Direct3D11::IDirect3DDevice>();
  }
};

class Capture {
  CaptureDevice& gpu_;
  wgc::GraphicsCaptureItem item_{nullptr};
  wgc::Direct3D11CaptureFramePool pool_{nullptr};
  wgc::GraphicsCaptureSession session_{nullptr};
  winrt::event_token closed_token_{};
  // Shared with the Closed handler, which runs on a system thread.
  std::shared_ptr<std::atomic<bool>> closed_=std::make_shared<std::atomic<bool>>(false);
  winrt::Windows::Graphics::SizeInt32 pool_size_{};
  ComPtr<ID3D11Texture2D> texture_;
  HWND window_=nullptr;
  HMONITOR monitor_=nullptr;
public:
  uint32_t width=0,height=0;
  // kind: 0 monitor (id is an HMONITOR), 1 window (id is an HWND).
  Capture(CaptureDevice& gpu,uint32_t kind,uint64_t id):gpu_(gpu) {
    require(wgc::GraphicsCaptureSession::IsSupported(),"Windows.Graphics.Capture is not supported on this system");
    auto interop=winrt::get_activation_factory<wgc::GraphicsCaptureItem,IGraphicsCaptureItemInterop>();
    if(kind==0) {
      monitor_=reinterpret_cast<HMONITOR>(uintptr_t(id));MONITORINFO info{sizeof(info)};
      require(GetMonitorInfoW(monitor_,&info)!=0,"Display is no longer available");
      check(interop->CreateForMonitor(monitor_,winrt::guid_of<wgc::IGraphicsCaptureItem>(),winrt::put_abi(item_)),"Capture display");
    } else {
      window_=reinterpret_cast<HWND>(uintptr_t(id));
      require(kind==1&&IsWindow(window_),"Window is no longer available");
      // A minimized window produces no frames. Restore it without stealing focus.
      if(IsIconic(window_))ShowWindow(window_,SW_SHOWNOACTIVATE);
      check(interop->CreateForWindow(window_,winrt::guid_of<wgc::IGraphicsCaptureItem>(),winrt::put_abi(item_)),"Capture window");
    }
    pool_size_=item_.Size();pool_size_.Width=std::max(pool_size_.Width,1);pool_size_.Height=std::max(pool_size_.Height,1);
    pool_=wgc::Direct3D11CaptureFramePool::CreateFreeThreaded(gpu_.winrt,wdx::DirectXPixelFormat::B8G8R8A8UIntNormalized,2,pool_size_);
    session_=pool_.CreateCaptureSession(item_);
    // The yellow capture border can only be removed on Windows 11; Windows 10 keeps it.
    try {
      if(winrt::Windows::Foundation::Metadata::ApiInformation::IsPropertyPresent(L"Windows.Graphics.Capture.GraphicsCaptureSession",L"IsBorderRequired"))session_.IsBorderRequired(false);
    } catch(const winrt::hresult_error&) {}
    session_.IsCursorCaptureEnabled(true);
    closed_token_=item_.Closed([closed=closed_](auto&&,auto&&){closed->store(true);});
    session_.StartCapture();
  }
  ~Capture() {
    try{item_.Closed(closed_token_);session_.Close();pool_.Close();}catch(const winrt::hresult_error&){}
  }
  Capture(const Capture&)=delete;Capture& operator=(const Capture&)=delete;
  ID3D11Texture2D* texture(){return texture_.Get();}
  HWND window() const {return window_;}
  HMONITOR monitor() const {return monitor_;}
  bool closed() const {return closed_->load()||(window_&&!IsWindow(window_));}
  // Screen rectangle the captured pixels correspond to, in physical pixels.
  bool bounds(RECT& r) const {
    if(window_)return SUCCEEDED(DwmGetWindowAttribute(window_,DWMWA_EXTENDED_FRAME_BOUNDS,&r,sizeof(r)));
    MONITORINFO info{sizeof(info)};if(!GetMonitorInfoW(monitor_,&info))return false;r=info.rcMonitor;return true;
  }
  // Copies the newest pending frame. Returns true when the texture changed.
  bool update() {
    if(closed())return false;
    wgc::Direct3D11CaptureFrame frame{nullptr};
    while(auto next=pool_.TryGetNextFrame())frame=next;
    if(!frame)return false;
    auto content=frame.ContentSize();
    if(content.Width<1||content.Height<1)return false;
    if(content.Width!=pool_size_.Width||content.Height!=pool_size_.Height) {
      // The source was resized. Frames of the new size follow after Recreate.
      pool_size_=content;frame=nullptr;
      pool_.Recreate(gpu_.winrt,wdx::DirectXPixelFormat::B8G8R8A8UIntNormalized,2,pool_size_);return false;
    }
    ComPtr<ID3D11Texture2D> source;
    check(frame.Surface().as<::Windows::Graphics::DirectX::Direct3D11::IDirect3DDxgiInterfaceAccess>()->GetInterface(IID_PPV_ARGS(&source)),"Capture frame texture");
    D3D11_TEXTURE2D_DESC desc{};source->GetDesc(&desc);
    const UINT w=std::min<UINT>(content.Width,desc.Width),h=std::min<UINT>(content.Height,desc.Height);
    if(!texture_||w!=width||h!=height) {
      texture_.Reset();desc.Width=w;desc.Height=h;desc.MipLevels=1;desc.ArraySize=1;desc.SampleDesc={1,0};
      desc.Usage=D3D11_USAGE_DEFAULT;desc.BindFlags=D3D11_BIND_SHADER_RESOURCE;desc.CPUAccessFlags=0;desc.MiscFlags=0;
      check(gpu_.device->CreateTexture2D(&desc,nullptr,&texture_),"Capture destination");width=w;height=h;
    }
    D3D11_BOX box{0,0,0,w,h,1};
    gpu_.context->CopySubresourceRegion(texture_.Get(),0,0,0,0,source.Get(),0,&box);
    return true;
  }
};
