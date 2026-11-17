// Thrown when SteamVR is not running. The worker then exits with the "waiting"
// code instead of launching SteamVR itself (an Overlay-type init would start it).
struct RuntimeUnavailable : std::runtime_error { using std::runtime_error::runtime_error; };
struct VR {
  explicit VR(const char* manifest){
    vr::EVRInitError e;
    vr::VR_Init(&e,vr::VRApplication_Background);
    if(e==vr::VRInitError_Init_NoServerForBackgroundApp)throw RuntimeUnavailable("SteamVR is not running");
    if(e!=vr::VRInitError_None)throw std::runtime_error(std::string("SteamVR probe: ")+vr::VR_GetVRInitErrorAsEnglishDescription(e));
    vr::VR_Shutdown();
    vr::VR_Init(&e,vr::VRApplication_Overlay);
    if(e!=vr::VRInitError_None)throw std::runtime_error(std::string("SteamVR: ")+vr::VR_GetVRInitErrorAsEnglishDescription(e));
    auto err=vr::VRInput()->SetActionManifestPath(manifest);if(err){vr::VR_Shutdown();throw std::runtime_error("SetActionManifestPath: "+std::to_string(err));}
  }
  ~VR(){vr::VR_Shutdown();}
};
static void transform(vr::VROverlayHandle_t h,const LoMatrix& m){for(float f:m.m)require(std::isfinite(f),"Nonfinite overlay transform");vr::HmdMatrix34_t matrix{};std::memcpy(matrix.m,m.m,sizeof(matrix));overlay_check(vr::VROverlay()->SetOverlayTransformAbsolute(h,vr::TrackingUniverseStanding,&matrix),"Overlay transform");}
static void transform_relative(vr::VROverlayHandle_t h,uint32_t device,const LoMatrix& m){for(float f:m.m)require(std::isfinite(f),"Nonfinite overlay transform");vr::HmdMatrix34_t matrix{};std::memcpy(matrix.m,m.m,sizeof(matrix));overlay_check(vr::VROverlay()->SetOverlayTransformTrackedDeviceRelative(h,device,&matrix),"Device-relative transform");}
static void visible(vr::VROverlayHandle_t h,bool show){overlay_check(show?vr::VROverlay()->ShowOverlay(h):vr::VROverlay()->HideOverlay(h),"Overlay visibility");}
// Handles 0..LO_PANELS-1 are panels (views, then capture slots); the last four
// are the two beams and two endpoint dots.
class Overlays {
public:
  std::array<vr::VROverlayHandle_t,LO_PANELS+4> handles{};
  std::array<bool,LO_PANELS> submitted{};
  bool aspect_checked=false;
  static constexpr int BEAM_TEXELS=8,DOT_TEXELS=32;
  vr::VROverlayHandle_t beam(unsigned hand) const {return handles[LO_PANELS+hand];}
  vr::VROverlayHandle_t dot(unsigned hand) const {return handles[LO_PANELS+2+hand];}
  void create(){
    auto* o=vr::VROverlay();
    for(size_t i=0;i<handles.size();++i){
      const char* lasers[]{"beam.left","beam.right","dot.left","dot.right"};const char* views[]{"keyboard","dashboard","wrist","bar"};
      std::string name=i<LO_VIEWS?views[i]:i<LO_PANELS?"capture."+std::to_string(i-LO_VIEWS):lasers[i-LO_PANELS];
      overlay_check(o->CreateOverlay(("wanderoverlay.world."+name).c_str(),("Wander Overlay "+name).c_str(),&handles[i]),"Create overlay");
      overlay_check(o->SetOverlayInputMethod(handles[i],vr::VROverlayInputMethod_None),"No runtime cursor");overlay_check(o->SetOverlayFlag(handles[i],vr::VROverlayFlags_VisibleInDashboard,false),"Dashboard visibility");
      // Captures draw first, HTML panels above them, lasers last. Equal sort
      // orders are distance-sorted by overlay center.
      if(i<LO_VIEWS){overlay_check(o->SetOverlaySortOrder(handles[i],1),"UI sort order");overlay_check(o->SetOverlayFlag(handles[i],vr::VROverlayFlags_IsPremultiplied,true),"UI alpha");}
      else if(i<LO_PANELS)overlay_check(o->SetOverlayFlag(handles[i],vr::VROverlayFlags_IgnoreTextureAlpha,true),"Capture alpha");
      else{overlay_check(o->SetOverlaySortOrder(handles[i],2),"Laser sort order");
        // White, tinted per click mode by lo_laser. The beam fades toward its
        // long edges so it reads as a round rod; the endpoint is a disc.
        std::vector<unsigned char> pixels;
        if(i-LO_PANELS<2){for(int x=0;x<BEAM_TEXELS;++x){float d=std::abs((x+0.5f)/BEAM_TEXELS*2-1);pixels.insert(pixels.end(),{255,255,255,(unsigned char)(235*std::sqrt(std::max(0.f,1-d*d)))});}
          overlay_check(o->SetOverlayRaw(handles[i],pixels.data(),BEAM_TEXELS,1,4),"Beam texture");}
        else{for(int y=0;y<DOT_TEXELS;++y)for(int x=0;x<DOT_TEXELS;++x){float r=std::hypot(x+0.5f-DOT_TEXELS/2,y+0.5f-DOT_TEXELS/2);pixels.insert(pixels.end(),{255,255,255,(unsigned char)(235*std::clamp(DOT_TEXELS/2-r,0.f,1.f))});}
          overlay_check(o->SetOverlayRaw(handles[i],pixels.data(),DOT_TEXELS,DOT_TEXELS,4),"Dot texture");}}
    }
  }
  ~Overlays(){auto* o=vr::VROverlay();if(!o)return;for(auto h:handles)if(h){o->HideOverlay(h);o->ClearOverlayTexture(h);o->DestroyOverlay(h);}}
  void submit(unsigned panel,ID3D11Texture2D* texture){vr::Texture_t t{texture,vr::TextureType_DirectX,vr::ColorSpace_Gamma};overlay_check(vr::VROverlay()->SetOverlayTexture(handles.at(panel),&t),"Submit texture");submitted.at(panel)=true;}
  void clear(unsigned panel){if(!submitted.at(panel))return;visible(handles[panel],false);overlay_check(vr::VROverlay()->ClearOverlayTexture(handles[panel]),"Clear texture");submitted[panel]=false;}
};
class Actions {
  vr::VRActionSetHandle_t set_=0;
  vr::VRActionHandle_t pose_=0,trigger_=0,grip_=0,right_=0,scroll_=0,keyboard_=0,toggle_=0;
  std::array<vr::VRInputValueHandle_t,2> hands_{};
  std::array<uint32_t,2> devices_{vr::k_unTrackedDeviceIndexInvalid,vr::k_unTrackedDeviceIndexInvalid};
  bool ready_=false;int last_error_=-999;
  void error(vr::EVRInputError e){if(int(e)!=last_error_){std::cout<<"Action readiness error="<<e<<std::endl;last_error_=e;}}
public:
  // Opens SteamVR's binding editor for this application's action set.
  void open_bindings(){require(ready_,"Controller actions are not ready");auto e=vr::VRInput()->OpenBindingUI(nullptr,set_,vr::k_ulInvalidInputValueHandle,false);if(e)throw std::runtime_error("OpenBindingUI: "+std::to_string(e));}
  uint32_t sample(LoHand* out,const vr::TrackedDevicePose_t* devices){
    auto* in=vr::VRInput();
    if(!ready_){vr::EVRInputError e=in->GetActionSetHandle("/actions/overlay",&set_);
      if(!e)e=in->GetActionHandle("/actions/overlay/in/pose",&pose_);
      if(!e)e=in->GetActionHandle("/actions/overlay/in/trigger",&trigger_);
      if(!e)e=in->GetActionHandle("/actions/overlay/in/grip",&grip_);
      if(!e)e=in->GetActionHandle("/actions/overlay/in/right",&right_);
      if(!e)e=in->GetActionHandle("/actions/overlay/in/keyboard",&keyboard_);
      if(!e)e=in->GetActionHandle("/actions/overlay/in/toggle",&toggle_);
      if(!e)e=in->GetActionHandle("/actions/overlay/in/scroll",&scroll_);
      if(!e)e=in->GetInputSourceHandle("/user/hand/left",&hands_[0]);if(!e)e=in->GetInputSourceHandle("/user/hand/right",&hands_[1]);
      if(e){error(e);return e;}ready_=true;
    }
    vr::VRActiveActionSet_t active{};active.ulActionSet=set_;active.nPriority=0;
    auto err=in->UpdateActionState(&active,sizeof(active),1);if(err){ready_=false;error(err);return err;}
    for(unsigned h=0;h<2;++h){auto& result=out[h];result.device=vr::k_unTrackedDeviceIndexInvalid;vr::InputPoseActionData_t p{};
      auto e=in->GetPoseActionDataRelativeToNow(pose_,vr::TrackingUniverseStanding,0,&p,sizeof(p),hands_[h]);vr::InputOriginInfo_t origin{};
      // The restrict-to-device argument already limits data to this hand.
      if(!e&&p.bActive&&p.pose.bPoseIsValid&&p.pose.bDeviceIsConnected&&in->GetOriginTrackedDeviceInfo(p.activeOrigin,&origin,sizeof(origin))==vr::VRInputError_None&&origin.trackedDeviceIndex<vr::k_unMaxTrackedDeviceCount) {
        const auto& raw=devices[origin.trackedDeviceIndex];
        if(raw.bPoseIsValid&&raw.bDeviceIsConnected){result.valid=1;result.device=origin.trackedDeviceIndex;std::memcpy(result.pose.m,p.pose.mDeviceToAbsoluteTracking.m,sizeof(result.pose));std::memcpy(result.raw.m,raw.mDeviceToAbsoluteTracking.m,sizeof(result.raw));}
      }
      auto digital=[&](vr::VRActionHandle_t a,uint32_t& value,uint32_t capability){vr::InputDigitalActionData_t d{};auto r=in->GetDigitalActionData(a,&d,sizeof(d),hands_[h]);if(!r&&d.bActive)result.actions_active|=capability;if(r)e=r;value=!r&&d.bActive&&d.bState;};
      if(result.device!=devices_[h]){devices_[h]=result.device;
        // Shows which default binding file SteamVR will pick for this controller.
        if(result.valid){char type[vr::k_unMaxPropertyStringSize]{};vr::VRSystem()->GetStringTrackedDeviceProperty(result.device,vr::Prop_ControllerType_String,type,sizeof(type));std::cout<<(h?"Right":"Left")<<" controller type: "<<type<<std::endl;}}
      digital(trigger_,result.trigger,1);digital(grip_,result.grip,2);digital(right_,result.right,4);digital(keyboard_,result.keyboard,16);digital(toggle_,result.toggle,32);
      vr::InputAnalogActionData_t a{};auto r=in->GetAnalogActionData(scroll_,&a,sizeof(a),hands_[h]);if(!r&&a.bActive)result.actions_active|=8;if(r)e=r;result.scroll=!r&&a.bActive&&std::isfinite(a.y)?a.y:0;if(e)err=e;
    }
    error(err);return err;
  }
};
