// Ultralight presentation resources and event dispatch; no keyboard policy.
// One view per HTML page. Each page sets `window.loReady = true` once loaded.
struct ViewSpec { const char* url; unsigned width,height; };
constexpr ViewSpec VIEWS[LO_VIEWS]{
  {"file:///ui/keyboard.html",1760,540},
  {"file:///ui/dashboard.html",1200,760},
  {"file:///ui/wrist.html",440,170},
  {"file:///ui/bar.html",700,190},
};
class UI {
  Log log_;
  Driver driver_;
  RefPtr<Renderer> renderer_;
  std::array<bool,LO_VIEWS> ready_{};
public:
  std::array<RefPtr<View>,LO_VIEWS> views;
  // Set when a view was repainted by the last frame() and needs resubmission.
  std::array<bool,LO_VIEWS> painted{};
  explicit UI(ultralight::platform::GPUContextD3D11& gpu):driver_(&gpu) {
    Platform::instance().set_config(Config{});Platform::instance().set_logger(&log_);
    Platform::instance().set_font_loader(GetPlatformFontLoader());Platform::instance().set_file_system(GetPlatformFileSystem("./assets/"));Platform::instance().set_gpu_driver(&driver_);
    renderer_=Renderer::Create();ViewConfig c;c.is_accelerated=true;c.is_transparent=true;c.enable_hidden_timer_throttling=true;c.initial_device_scale=1;c.initial_focus=false;
    for(size_t i=0;i<views.size();++i){views[i]=renderer_->CreateView(VIEWS[i].width,VIEWS[i].height,c,nullptr);views[i]->LoadURL(VIEWS[i].url);}
  }
  ~UI(){for(auto& v:views)v=nullptr;renderer_=nullptr;Platform::instance().set_gpu_driver(nullptr);Platform::instance().set_logger(nullptr);}
  void frame() {
    renderer_->Update();renderer_->RefreshDisplay(0);
    // Read after RefreshDisplay: script and DOM changes only request a repaint from that call.
    for(size_t i=0;i<views.size();++i)painted[i]=views[i]->needs_paint();
    renderer_->Render();driver_.DrawCommandList();
  }
  std::string script(unsigned view,const char* source){String error;auto r=views.at(view)->EvaluateScript(source,&error);if(!error.empty())throw std::runtime_error(error.utf8().data());return r.utf8().data();}
  bool ready(unsigned view){if(!ready_[view])ready_[view]=script(view,"window.loReady===true?'ready':''")=="ready"&&bool(texture(view));return ready_[view];}
  ComPtr<ID3D11Texture2D> texture(unsigned view){auto t=views.at(view)->render_target();return t.is_empty?ComPtr<ID3D11Texture2D>{}:driver_.texture(t.texture_id);}
  vr::VRTextureBounds_t bounds(unsigned view){auto uv=views.at(view)->render_target().uv_coords;return {uv.left,uv.top,uv.right,uv.bottom};}
  // 1 + index of the view whose editable element has keyboard focus, else 0.
  uint32_t focused(){for(size_t i=0;i<views.size();++i)if(views[i]->HasInputFocus())return uint32_t(i+1);return 0;}
  void key(View& view,const LoKey& k) {
    KeyEvent e;e.type=KeyEvent::kType_RawKeyDown;e.modifiers=k.modifiers;e.virtual_key_code=k.vk;e.native_key_code=k.vk;
    GetKeyIdentifierFromVirtualKeyCode(k.vk,e.key_identifier);view.FireKeyEvent(e);
    if(k.character && !(k.modifiers&7)){char text[2]{static_cast<char>(k.character),0};e.type=KeyEvent::kType_Char;e.text=text;e.unmodified_text=text;view.FireKeyEvent(e);}
    e.type=KeyEvent::kType_KeyUp;e.text="";e.unmodified_text="";view.FireKeyEvent(e);
  }
};
