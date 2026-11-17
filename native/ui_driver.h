// The only Ultralight-specific GPU adapter. The SDK sources are left untouched.
class Driver : public ultralight::platform::GPUDriverD3D11 {
public:
  using GPUDriverD3D11::GPUDriverD3D11;
  ComPtr<ID3D11Texture2D> texture(uint32_t id) {
    auto* srv=GetTextureSRV(id); // Resolves MSAA when necessary.
    if (!srv) return {};
    ComPtr<ID3D11Resource> resource; srv->GetResource(&resource);
    ComPtr<ID3D11Texture2D> result; check(resource.As(&result),"UI texture"); return result;
  }
};
class Log : public Logger {
  void LogMessage(LogLevel level, const String& message) override {
    std::cerr << "[ultralight " << static_cast<int>(level) << "] " << message.utf8().data() << '\n';
  }
};

