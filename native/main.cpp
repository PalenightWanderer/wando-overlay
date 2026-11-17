#include "platform.h"
#include <windows.h>
#include <dwmapi.h>
#include <tlhelp32.h>
#include <d3d11.h>
#include <dxgi1_2.h>
#include <wrl/client.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Foundation.Metadata.h>
#include <winrt/Windows.Graphics.Capture.h>
#include <winrt/Windows.Graphics.DirectX.Direct3D11.h>
#include <windows.graphics.capture.interop.h>
#include <windows.graphics.directx.direct3d11.interop.h>
#include <Ultralight/Ultralight.h>
#include <AppCore/AppCore.h>
#include <Platform/gpu/d3d11/GPUContextD3D11.h>
#include <Platform/gpu/d3d11/GPUDriverD3D11.h>
#include <openvr.h>
#include <algorithm>
#include <array>
#include <atomic>
#include <chrono>
#include <cmath>
#include <cstring>
#include <iostream>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <string>
#include <type_traits>
#include <vector>
using Microsoft::WRL::ComPtr;
using namespace ultralight;
void check(HRESULT hr,const char* op){if(FAILED(hr)){std::ostringstream s;s<<op<<": HRESULT 0x"<<std::hex<<hr;throw std::runtime_error(s.str());}}
void overlay_check(vr::EVROverlayError e,const char* op){if(e!=vr::VROverlayError_None)throw std::runtime_error(std::string(op)+": "+vr::VROverlay()->GetOverlayErrorNameFromEnum(e));}
static void require(bool value,const char* message){if(!value)throw std::runtime_error(message);}
#include "ui_driver.h"
#include "capture.h"
#include "sources.h"
#include "ui.h"
#include "vr.h"

// Resources only: Rust decides when to poll, render, capture and dispatch input.
struct Adapter {
  std::unique_ptr<VR> vr;
  ultralight::platform::GPUContextD3D11 gpu;
  UI ui;
  CaptureDevice capture_gpu;
  std::array<std::unique_ptr<Capture>,LO_CAPTURES> captures;
  Actions actions;
  uint64_t session_start=0;
  uint32_t painted=0; // views repainted by the last lo_frame
  // Last requested placement per panel. Applied at once when the panel has a
  // texture, otherwise when its first texture is submitted.
  struct Placement { LoMatrix matrix{};float width=1;uint32_t device=LO_NO_DEVICE;bool show=false; };
  std::array<Placement,LO_PANELS> placements;
  Overlays overlays; // destroyed before captures, UI, GPU and VR
  Adapter(bool use_vr,const char* manifest):vr(use_vr?std::make_unique<VR>(manifest):nullptr),ui(gpu) {
    require(gpu.device()!=nullptr,"No D3D11 device");if(vr){overlays.create();session_start=vrserver_start();}
  }
};
static std::unique_ptr<Adapter> app;
static void place(unsigned panel) {
  auto h=app->overlays.handles[panel];const auto& p=app->placements[panel];
  // A panel without a texture stays hidden; SteamVR would draw nothing useful.
  if(!p.show||!app->overlays.submitted[panel]){visible(h,false);return;}
  if(p.device==LO_NO_DEVICE)transform(h,p.matrix);else transform_relative(h,p.device,p.matrix);
  overlay_check(vr::VROverlay()->SetOverlayWidthInMeters(h,p.width),"Panel width");visible(h,true);
}
static void submit(unsigned panel,ID3D11Texture2D* texture) {
  const bool first=!app->overlays.submitted[panel];app->overlays.submit(panel,texture);if(first)place(panel);
}
static std::string last_error;
template<class F> int32_t guarded(F&& f) noexcept {
  try{f();return 0;}catch(const std::exception& e){last_error=e.what();return -1;}
  catch(const winrt::hresult_error& e){last_error="Windows Runtime: "+winrt::to_string(e.message());return -1;}
  catch(...){last_error="Unknown native adapter exception";return -1;}
}
static void copy_out(const std::string& text,char* result,uint32_t capacity){require(result&&capacity>text.size(),"Result buffer too small");std::memcpy(result,text.c_str(),text.size()+1);}
static_assert(sizeof(LoMatrix)==48 && sizeof(LoHand)==132 && sizeof(LoCapture)==16 && sizeof(LoFrame)==488);
static_assert(sizeof(LoInput)==24 && sizeof(LoKey)==12 && std::is_standard_layout_v<LoFrame>);
extern "C" {
const char* lo_error(void){return last_error.c_str();}
int32_t lo_create(uint32_t vr,const char* manifest){
  try{
    require(!app,"Adapter already created");SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    try{winrt::init_apartment(winrt::apartment_type::multi_threaded);}catch(const winrt::hresult_error&){} // already initialized by the host
    app=std::make_unique<Adapter>(vr!=0,manifest);return 0;
  }
  catch(const RuntimeUnavailable& e){last_error=e.what();return -2;}
  catch(const std::exception& e){last_error=e.what();return -1;}
  catch(const winrt::hresult_error& e){last_error="Windows Runtime: "+winrt::to_string(e.message());return -1;}
  catch(...){last_error="Unknown native adapter exception";return -1;}
}
int32_t lo_destroy(void){return guarded([]{app.reset();});}
int32_t lo_script(uint32_t view,const char* source,char* result,uint32_t capacity){return guarded([&]{require(app&&source&&view<LO_VIEWS,"Invalid script request");auto s=app->ui.script(view,source);if(result)copy_out(s,result,capacity);});}
int32_t lo_sample(LoFrame* f,uint32_t size){return guarded([&]{
  require(bool(app)&&f&&size==sizeof(LoFrame),"Frame ABI size mismatch");*f={};f->size=sizeof(*f);
  for(unsigned i=0;i<LO_CAPTURES;++i)if(auto& c=app->captures[i]){f->captures[i]={c->closed()?2u:1u,c->texture()!=nullptr,c->width,c->height};}
  for(unsigned i=0;i<LO_VIEWS;++i)if(app->ui.ready(i))f->ui_ready|=1u<<i;
  f->local_focus=app->ui.focused();f->painted=app->painted;
  SYSTEMTIME now;GetLocalTime(&now);f->hour=now.wHour;f->minute=now.wMinute;
  if(!app->vr)return;
  f->session_seconds=seconds_since(app->session_start);
  // Manifest was installed before this first event poll or action update.
  vr::VREvent_t e{};while(vr::VRSystem()->PollNextEvent(&e,sizeof(e))) {
    if(e.eventType==vr::VREvent_Quit){f->quit=1;vr::VRSystem()->AcknowledgeQuit_Exiting();}
    if(e.eventType==vr::VREvent_ChaperoneUniverseHasChanged)f->recenter=1;
  }
  f->dashboard=vr::VROverlay()->IsDashboardVisible();
  std::array<vr::TrackedDevicePose_t,vr::k_unMaxTrackedDeviceCount> poses{};
  vr::VRSystem()->GetDeviceToAbsoluteTrackingPose(vr::TrackingUniverseStanding,0,poses.data(),uint32_t(poses.size()));
  const auto& head=poses[0];f->hmd_valid=head.bDeviceIsConnected&&head.bPoseIsValid;
  if(f->hmd_valid)std::memcpy(f->hmd.m,head.mDeviceToAbsoluteTracking.m,sizeof(f->hmd));
  f->action_error=app->actions.sample(f->hands,poses.data());
});}
int32_t lo_frame(uint32_t capture,uint32_t render){return guarded([&]{
  require(bool(app),"No adapter");app->painted=0;
  if(capture) {
    bool copied=false;
    for(unsigned i=0;i<LO_CAPTURES;++i)if(auto& c=app->captures[i]){if(c->update()){copied=true;if(app->vr)submit(LO_VIEWS+i,c->texture());}}
    if(copied&&!app->vr)app->capture_gpu.context->Flush();
  }
  if(render) {
    app->ui.frame();
    for(unsigned i=0;i<LO_VIEWS;++i)if(app->ui.painted[i])app->painted|=1u<<i;
    for(unsigned i=0;i<LO_VIEWS&&app->vr;++i) {
      if(!app->ui.painted[i]&&app->overlays.submitted[i])continue;
      auto t=app->ui.texture(i);if(!t)continue;
      auto b=app->ui.bounds(i);overlay_check(vr::VROverlay()->SetOverlayTextureBounds(app->overlays.handles[i],&b),"UI bounds");submit(i,t.Get());
    }
  }
});}
int32_t lo_panel(uint32_t panel,uint32_t device,const LoMatrix* m,float width,uint32_t show){return guarded([&]{
  require(app&&m&&panel<LO_PANELS&&std::isfinite(width)&&width>0,"Invalid panel");
  if(panel<LO_VIEWS)app->ui.views[panel]->set_visible(show!=0);
  require(device==LO_NO_DEVICE||device<vr::k_unMaxTrackedDeviceCount,"Invalid panel device");
  app->placements[panel]={*m,width,device,show!=0};if(app->vr)place(panel);
});}
int32_t lo_style(uint32_t panel,float curvature,float alpha){return guarded([&]{
  require(app&&panel<LO_PANELS&&std::isfinite(curvature)&&std::isfinite(alpha),"Invalid style");if(!app->vr)return;
  if(panel>=LO_VIEWS)overlay_check(vr::VROverlay()->SetOverlayCurvature(app->overlays.handles[panel],std::clamp(curvature,0.f,1.f)),"Panel curvature");
  overlay_check(vr::VROverlay()->SetOverlayAlpha(app->overlays.handles[panel],std::clamp(alpha,0.f,1.f)),"Panel opacity");
});}
int32_t lo_laser(uint32_t hand,uint32_t device,const LoMatrix* m,const LoMatrix* endpoint,float length,uint32_t show,uint32_t right){return guarded([&]{
  require(app&&app->vr&&m&&endpoint&&hand<2,"Invalid laser");auto beam=app->overlays.beam(hand),dot=app->overlays.dot(hand);
  if(show){require(std::isfinite(length)&&length>0&&device<vr::k_unMaxTrackedDeviceCount,"Invalid beam");
    // Attached to the controller so the compositor applies its latest pose (no
    // trailing behind fast hand motion). The beam texture is BEAM_TEXELS x 1,
    // so its world height is width / (BEAM_TEXELS * texel aspect): the aspect
    // sets the beam length.
    const float width=0.004f,aspect=width/(Overlays::BEAM_TEXELS*length);
    transform_relative(beam,device,*m);
    overlay_check(vr::VROverlay()->SetOverlayWidthInMeters(beam,width),"Beam width");
    overlay_check(vr::VROverlay()->SetOverlayTexelAspect(beam,aspect),"Beam length");
    if(!app->overlays.aspect_checked){float applied=0;overlay_check(vr::VROverlay()->GetOverlayTexelAspect(beam,&applied),"Check beam aspect");
      if(std::abs(applied/aspect-1)>0.01f)std::cerr<<"Beam texel aspect was adjusted by the runtime; drawn length may differ\n";app->overlays.aspect_checked=true;}
    // Blue clicks with the left mouse button, orange with the right.
    for(auto h:{beam,dot})overlay_check(right?vr::VROverlay()->SetOverlayColor(h,1.f,0.62f,0.2f):vr::VROverlay()->SetOverlayColor(h,0.31f,0.82f,1.f),"Laser colour");
    transform_relative(dot,device,*endpoint);overlay_check(vr::VROverlay()->SetOverlayWidthInMeters(dot,0.013f),"Dot width");}
  visible(beam,show!=0);visible(dot,show!=0);
});}
int32_t lo_local_key(const LoKey* k){return guarded([&]{
  require(app&&k,"Invalid key");auto focused=app->ui.focused();require(focused!=0,"Local editable target no longer focused");
  app->ui.key(*app->ui.views[focused-1],*k);
});}
int32_t lo_mouse(uint32_t view,int32_t x,int32_t y,uint32_t kind){return guarded([&]{
  require(app&&kind<=3&&view<LO_VIEWS,"Invalid UI pointer event");auto& target=*app->ui.views[view];
  if(kind==3){ScrollEvent s{};s.type=ScrollEvent::kType_ScrollByPixel;s.delta_x=0;s.delta_y=y;target.FireScrollEvent(s);return;}
  MouseEvent e{};e.x=x;e.y=y;e.type=kind==0?MouseEvent::kType_MouseMoved:kind==1?MouseEvent::kType_MouseDown:MouseEvent::kType_MouseUp;
  e.button=kind==0?MouseEvent::kButton_None:MouseEvent::kButton_Left;
  // Only one view owns keyboard focus at a time.
  if(kind==1){for(auto& other:app->ui.views)if(other.get()!=&target)other->Unfocus();target.Focus();}
  target.FireMouseEvent(e);
});}
int32_t lo_inject(const LoInput* e){return guarded([&]{
  require(app&&app->vr&&e,"Input unavailable outside live VR session");INPUT i{};
  auto slot=[&]()->Capture&{require(e->code<LO_CAPTURES&&app->captures[e->code]&&app->captures[e->code]->texture(),"Capture unavailable");return *app->captures[e->code];};
  switch(e->kind) {
    case 0:i.type=INPUT_KEYBOARD;i.ki.wVk=WORD(e->code);i.ki.dwFlags=(e->down?0:KEYEVENTF_KEYUP)|(e->extended?KEYEVENTF_EXTENDEDKEY:0);break;
    case 1:require(e->code<2,"Invalid mouse button");i.type=INPUT_MOUSE;i.mi.dwFlags=e->code?(e->down?MOUSEEVENTF_RIGHTDOWN:MOUSEEVENTF_RIGHTUP):(e->down?MOUSEEVENTF_LEFTDOWN:MOUSEEVENTF_LEFTUP);break;
    case 2:{auto& c=slot();RECT r{};require(c.bounds(r)&&r.right>r.left&&r.bottom>r.top,"Capture source has no screen area");
      // Capture pixels map proportionally onto the source's screen rectangle.
      const double sx=r.left+(e->x+0.5)*(r.right-r.left)/c.width,sy=r.top+(e->y+0.5)*(r.bottom-r.top)/c.height;
      i.type=INPUT_MOUSE;i.mi.dwFlags=MOUSEEVENTF_MOVE|MOUSEEVENTF_ABSOLUTE|MOUSEEVENTF_VIRTUALDESK;
      auto absolute=[](double pixel,int origin,int extent){return LONG(std::lround(std::clamp((pixel-origin)/std::max(1,extent-1),0.0,1.0)*65535));};
      i.mi.dx=absolute(sx,GetSystemMetrics(SM_XVIRTUALSCREEN),GetSystemMetrics(SM_CXVIRTUALSCREEN));
      i.mi.dy=absolute(sy,GetSystemMetrics(SM_YVIRTUALSCREEN),GetSystemMetrics(SM_CYVIRTUALSCREEN));break;}
    case 3:i.type=INPUT_MOUSE;i.mi.dwFlags=MOUSEEVENTF_WHEEL;i.mi.mouseData=DWORD(e->y);break;
    case 4:require(e->code>0&&e->code<=0xffff,"Invalid Unicode unit");i.type=INPUT_KEYBOARD;i.ki.wScan=WORD(e->code);i.ki.dwFlags=KEYEVENTF_UNICODE|(e->down?0:KEYEVENTF_KEYUP);break;
    case 5:{
      // Best effort: Windows may refuse to change the foreground window. The
      // click that follows still lands on whatever is under the cursor.
      if(HWND w=slot().window()){if(IsIconic(w))ShowWindow(w,SW_RESTORE);if(GetForegroundWindow()!=w)SetForegroundWindow(w);}
      return;}
    default:throw std::runtime_error("Invalid input kind");
  }
  if(SendInput(1,&i,sizeof(i))!=1)throw std::runtime_error("SendInput rejected (UIPI or input restriction possible), error "+std::to_string(GetLastError()));
});}
int32_t lo_sources(char* result,uint32_t capacity){return guarded([&]{require(bool(app),"No adapter");copy_out(list_sources(),result,capacity);});}
int32_t lo_capture_open(uint32_t slot,uint32_t kind,uint64_t id){return guarded([&]{
  // Display id 0 selects the primary display (smoke test and diagnostic).
  if(kind==0&&id==0)id=reinterpret_cast<uintptr_t>(MonitorFromPoint(POINT{0,0},MONITOR_DEFAULTTOPRIMARY));
  require(app&&slot<LO_CAPTURES&&kind<2,"Invalid capture request");require(!app->captures[slot],"Capture slot is in use");
  app->captures[slot]=std::make_unique<Capture>(app->capture_gpu,kind,id);
});}
int32_t lo_capture_close(uint32_t slot){return guarded([&]{
  require(app&&slot<LO_CAPTURES,"Invalid capture slot");if(app->vr)app->overlays.clear(LO_VIEWS+slot);app->captures[slot].reset();
});}
int32_t lo_capture_identity(uint32_t slot,char* result,uint32_t capacity){return guarded([&]{
  require(app&&slot<LO_CAPTURES&&app->captures[slot],"No capture in that slot");auto& c=*app->captures[slot];
  copy_out(c.window()?window_identity(c.window()):monitor_identity(c.monitor()),result,capacity);
});}
int32_t lo_source_find(uint32_t kind,const char* identity,uint64_t* id){return guarded([&]{
  require(app&&identity&&id&&kind<2,"Invalid source search");std::vector<HWND> windows;std::vector<HMONITOR> monitors;
  for(auto& c:app->captures)if(c){if(c->window())windows.push_back(c->window());else monitors.push_back(c->monitor());}
  *id=*identity==0?0:kind==1?find_window_source(identity,windows):find_monitor_source(identity,monitors);
});}
int32_t lo_capture_placeholder(uint32_t slot,const char* text,uint32_t width,uint32_t height){return guarded([&]{
  require(app&&text&&slot<LO_CAPTURES&&!app->captures[slot]&&width>0&&height>0,"Invalid placeholder request");if(!app->vr)return;
  // Drawn at most 960 pixels wide in the source's aspect, so the text is not stretched.
  const int w=960,h=std::clamp(int(960.0*height/width),120,2160);
  BITMAPINFO info{};info.bmiHeader={sizeof(BITMAPINFOHEADER),w,-h,1,32,BI_RGB};void* bits=nullptr;
  HDC dc=CreateCompatibleDC(nullptr);HBITMAP bitmap=CreateDIBSection(dc,&info,DIB_RGB_COLORS,&bits,nullptr,0);
  require(dc&&bitmap&&bits,"Placeholder bitmap");auto old=SelectObject(dc,bitmap);
  RECT area{0,0,w,h};HBRUSH grey=CreateSolidBrush(RGB(58,62,70));FillRect(dc,&area,grey);DeleteObject(grey);
  HFONT font=CreateFontW(110,0,0,0,FW_SEMIBOLD,0,0,0,DEFAULT_CHARSET,0,0,CLEARTYPE_QUALITY,0,L"Segoe UI");auto old_font=SelectObject(dc,font);
  SetBkMode(dc,TRANSPARENT);SetTextColor(dc,RGB(215,220,228));
  int length=MultiByteToWideChar(CP_UTF8,0,text,-1,nullptr,0);std::wstring wide(length,L'\0');MultiByteToWideChar(CP_UTF8,0,text,-1,wide.data(),length);
  DrawTextW(dc,wide.c_str(),-1,&area,DT_CENTER|DT_VCENTER|DT_SINGLELINE|DT_NOPREFIX);GdiFlush();
  // Grey text on grey: red and blue need no swap for SetOverlayRaw's RGBA. GDI leaves alpha at zero.
  auto* pixel=static_cast<uint8_t*>(bits);for(int i=0;i<w*h;++i)pixel[i*4+3]=255;
  auto error=vr::VROverlay()->SetOverlayRaw(app->overlays.handles[LO_VIEWS+slot],bits,w,h,4);
  SelectObject(dc,old_font);DeleteObject(font);SelectObject(dc,old);DeleteObject(bitmap);DeleteDC(dc);
  overlay_check(error,"Placeholder texture");app->overlays.submitted[LO_VIEWS+slot]=true;place(LO_VIEWS+slot);
});}
int32_t lo_open_bindings(void){return guarded([]{require(app&&app->vr,"SteamVR is not connected");app->actions.open_bindings();});}
int32_t lo_smoke_image(uint32_t view,const char* path){return guarded([&]{
  require(app&&path&&view<LO_VIEWS,"Invalid image request");auto texture=app->ui.texture(view);require(bool(texture),"No UI texture");
  D3D11_TEXTURE2D_DESC d{};texture->GetDesc(&d);d.Usage=D3D11_USAGE_STAGING;d.BindFlags=0;d.MiscFlags=0;d.CPUAccessFlags=D3D11_CPU_ACCESS_READ;
  ComPtr<ID3D11Texture2D> staging;check(app->gpu.device()->CreateTexture2D(&d,nullptr,&staging),"Smoke staging");app->gpu.immediate_context()->CopyResource(staging.Get(),texture.Get());D3D11_MAPPED_SUBRESOURCE mapped{};
  check(app->gpu.immediate_context()->Map(staging.Get(),0,D3D11_MAP_READ,0,&mapped),"Smoke readback");
  auto bitmap=Bitmap::Create(d.Width,d.Height,BitmapFormat::BGRA8_UNORM_SRGB,mapped.RowPitch,mapped.pData,size_t(mapped.RowPitch)*d.Height);
  app->gpu.immediate_context()->Unmap(staging.Get(),0);require(bitmap->WritePNG(path),"Write smoke image");
});}
}
