// Lists capturable displays and windows as JSON for the dashboard's Add screen:
// {"monitors":[{"id":"..","name":"..","w":1920,"h":1080,"primary":true}],
//  "windows":[{"id":"..","title":"..","app":"..","minimized":false}]}
// Ids are HMONITOR/HWND values as decimal strings, passed back to lo_capture_open.
static std::string utf8(const wchar_t* text,int length=-1) {
  int size=WideCharToMultiByte(CP_UTF8,0,text,length,nullptr,0,nullptr,nullptr);if(size<=0)return {};
  std::string out(size,'\0');WideCharToMultiByte(CP_UTF8,0,text,length,out.data(),size,nullptr,nullptr);
  if(length<0&&!out.empty())out.pop_back();return out;
}
static std::string json_string(const std::string& text) {
  std::string out="\"";
  for(unsigned char c:text) {
    if(c=='"'||c=='\\'){out+='\\';out+=char(c);}
    else if(c<0x20){char buffer[8];snprintf(buffer,sizeof(buffer),"\\u%04x",c);out+=buffer;}
    else out+=char(c);
  }
  return out+'"';
}
struct SourceList { std::string monitors,windows;int monitor_count=0; };
static BOOL CALLBACK list_monitor(HMONITOR monitor,HDC,LPRECT,LPARAM data) {
  auto& list=*reinterpret_cast<SourceList*>(data);MONITORINFO info{sizeof(info)};
  if(!GetMonitorInfoW(monitor,&info))return TRUE;
  if(!list.monitors.empty())list.monitors+=',';
  list.monitors+="{\"id\":\""+std::to_string(reinterpret_cast<uintptr_t>(monitor))+"\",\"name\":\"Display "+std::to_string(++list.monitor_count)
    +"\",\"w\":"+std::to_string(info.rcMonitor.right-info.rcMonitor.left)+",\"h\":"+std::to_string(info.rcMonitor.bottom-info.rcMonitor.top)
    +",\"primary\":"+((info.dwFlags&MONITORINFOF_PRIMARY)?"true":"false")+"}";
  return TRUE;
}
// The same windows Alt+Tab offers: visible, titled, unowned, not tool windows,
// not cloaked (other virtual desktops, suspended store apps) and not our own.
static bool capturable(HWND window) {
  if(!IsWindowVisible(window)||GetWindow(window,GW_OWNER)||(GetWindowLongPtrW(window,GWL_EXSTYLE)&WS_EX_TOOLWINDOW))return false;
  DWORD cloaked=0;if(SUCCEEDED(DwmGetWindowAttribute(window,DWMWA_CLOAKED,&cloaked,sizeof(cloaked)))&&cloaked)return false;
  DWORD process=0;GetWindowThreadProcessId(window,&process);if(process==GetCurrentProcessId())return false;
  if(GetWindowTextLengthW(window)<=0)return false;
  wchar_t class_name[64]{};GetClassNameW(window,class_name,64);return wcscmp(class_name,L"Progman")!=0;
}
static std::string window_title(HWND window){wchar_t title[160];int length=GetWindowTextW(window,title,160);return length>0?utf8(title,length):std::string{};}
static std::string window_class(HWND window){wchar_t name[128]{};GetClassNameW(window,name,128);return utf8(name);}
static std::string window_app(HWND window) {
  DWORD process=0;GetWindowThreadProcessId(window,&process);std::string app;
  if(HANDLE handle=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,FALSE,process)) {
    wchar_t path[MAX_PATH];DWORD size=MAX_PATH;
    if(QueryFullProcessImageNameW(handle,0,path,&size)){const wchar_t* name=wcsrchr(path,L'\\');app=utf8(name?name+1:path);}
    CloseHandle(handle);
  }
  return app;
}
static BOOL CALLBACK list_window(HWND window,LPARAM data) {
  auto& list=*reinterpret_cast<SourceList*>(data);if(!capturable(window))return TRUE;
  if(!list.windows.empty())list.windows+=',';
  list.windows+="{\"id\":\""+std::to_string(reinterpret_cast<uintptr_t>(window))+"\",\"title\":"+json_string(window_title(window))
    +",\"app\":"+json_string(window_app(window))+",\"minimized\":"+(IsIconic(window)?"true":"false")+"}";
  return TRUE;
}
// Identities name a source across restarts, where handles change.
// A window is "app<TAB>class<TAB>title".
// A display is "interface path<TAB>GDI name", for example
// "\\\\?\\DISPLAY#GSM5B09#5&2f0c&0&UID4352#{...}<TAB>\\\\.\\DISPLAY1". The GDI name is
// renumbered when displays come and go (virtual display drivers do this a
// lot), so the interface path identifies the display and the GDI name is
// only kept for showing to the user.
static std::string monitor_identity(HMONITOR monitor) {
  MONITORINFOEXW info{};info.cbSize=sizeof(info);if(!GetMonitorInfoW(monitor,&info))return {};
  DISPLAY_DEVICEW device{};device.cb=sizeof(device);std::string path;
  if(EnumDisplayDevicesW(info.szDevice,0,&device,EDD_GET_DEVICE_INTERFACE_NAME))path=utf8(device.DeviceID);
  return path+'\t'+utf8(info.szDevice);
}
// The model part of an interface path: "GSM5B09" in "\\\\?\\DISPLAY#GSM5B09#...".
static std::string monitor_model(const std::string& path) {
  auto first=path.find('#');if(first==std::string::npos)return {};
  auto second=path.find('#',first+1);return second==std::string::npos?std::string{}:path.substr(first+1,second-first-1);
}
static std::string window_identity(HWND window) {
  auto title=window_title(window);std::replace(title.begin(),title.end(),'\t',' ');
  return window_app(window)+'\t'+window_class(window)+'\t'+title;
}
struct SourceSearch { std::string identity,app_class;const std::vector<HWND>* taken;uint64_t found=0;bool exact=false; };
struct MonitorSearch { std::string path,model,name;const std::vector<HMONITOR>* taken;HMONITOR exact=nullptr,same_model=nullptr,same_name=nullptr; };
static BOOL CALLBACK find_monitor(HMONITOR monitor,HDC,LPRECT,LPARAM data) {
  auto& search=*reinterpret_cast<MonitorSearch*>(data);auto identity=monitor_identity(monitor);
  auto tab=identity.find('\t');if(tab==std::string::npos)return TRUE;
  auto path=identity.substr(0,tab),name=identity.substr(tab+1);
  if(!path.empty()&&path==search.path){search.exact=monitor;return FALSE;}
  if(std::find(search.taken->begin(),search.taken->end(),monitor)!=search.taken->end())return TRUE;
  if(!search.same_model&&!search.model.empty()&&monitor_model(path)==search.model)search.same_model=monitor;
  if(!search.same_name&&name==search.name)search.same_name=monitor;
  return TRUE;
}
// The same interface path wins. Otherwise a display of the same model that is
// not captured yet (the path's instance part can change with the port or the
// driver). The GDI name alone only matches identities saved without a path.
static uint64_t find_monitor_source(const std::string& identity,const std::vector<HMONITOR>& taken) {
  auto tab=identity.find('\t');MonitorSearch search{};search.taken=&taken;
  if(tab==std::string::npos)search.name=identity;else{search.path=identity.substr(0,tab);search.name=identity.substr(tab+1);}
  search.model=monitor_model(search.path);
  EnumDisplayMonitors(nullptr,nullptr,find_monitor,reinterpret_cast<LPARAM>(&search));
  HMONITOR found=search.exact?search.exact:search.same_model?search.same_model:search.path.empty()?search.same_name:nullptr;
  return reinterpret_cast<uintptr_t>(found);
}
// Same application and window class is required. Among those the same title
// wins; otherwise the first one, since titles change with the open document.
static BOOL CALLBACK find_window(HWND window,LPARAM data) {
  auto& search=*reinterpret_cast<SourceSearch*>(data);
  if(!capturable(window)||std::find(search.taken->begin(),search.taken->end(),window)!=search.taken->end())return TRUE;
  auto identity=window_identity(window);
  if(identity==search.identity){search.found=reinterpret_cast<uintptr_t>(window);search.exact=true;return FALSE;}
  if(!search.found&&identity.compare(0,search.app_class.size(),search.app_class)==0)search.found=reinterpret_cast<uintptr_t>(window);
  return TRUE;
}
// Returns 0 when no current source matches. `taken` windows are skipped so two
// saved windows of one application do not resolve to the same window.
static uint64_t find_window_source(const std::string& identity,const std::vector<HWND>& taken) {
  SourceSearch search{identity,{},&taken};
  auto first=identity.find('\t');if(first==std::string::npos)return 0;
  auto second=identity.find('\t',first+1);if(second==std::string::npos)return 0;
  search.app_class=identity.substr(0,second+1);EnumWindows(find_window,reinterpret_cast<LPARAM>(&search));return search.found;
}
static std::string list_sources() {
  SourceList list;EnumDisplayMonitors(nullptr,nullptr,list_monitor,reinterpret_cast<LPARAM>(&list));
  EnumWindows(list_window,reinterpret_cast<LPARAM>(&list));
  return "{\"monitors\":["+list.monitors+"],\"windows\":["+list.windows+"]}";
}
// Seconds since vrserver.exe started, i.e. the length of this SteamVR session.
static uint64_t vrserver_start() {
  HANDLE snapshot=CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS,0);if(snapshot==INVALID_HANDLE_VALUE)return 0;
  PROCESSENTRY32W entry{sizeof(entry)};uint64_t start=0;
  for(BOOL more=Process32FirstW(snapshot,&entry);more&&!start;more=Process32NextW(snapshot,&entry)) {
    if(_wcsicmp(entry.szExeFile,L"vrserver.exe"))continue;
    if(HANDLE process=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,FALSE,entry.th32ProcessID)) {
      FILETIME created,exited,kernel,user;
      if(GetProcessTimes(process,&created,&exited,&kernel,&user))start=(uint64_t(created.dwHighDateTime)<<32)|created.dwLowDateTime;
      CloseHandle(process);
    }
  }
  CloseHandle(snapshot);return start;
}
static uint32_t seconds_since(uint64_t start) {
  if(!start)return 0;FILETIME now;GetSystemTimeAsFileTime(&now);
  uint64_t current=(uint64_t(now.dwHighDateTime)<<32)|now.dwLowDateTime;
  return current>start?uint32_t((current-start)/10000000ull):0;
}
