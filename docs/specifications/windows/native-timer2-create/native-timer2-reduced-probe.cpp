#pragma comment(lib, "advapi32.lib")
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <winternl.h>
#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
using Create = LONG(NTAPI *)(PHANDLE, PULONG, POBJECT_ATTRIBUTES, ULONG, ACCESS_MASK);
using Query = LONG(NTAPI *)(HANDLE, ULONG, PVOID, ULONG, PULONG);
using Close = LONG(NTAPI *)(HANDLE);
static Create create_timer;
static Query query_timer, query_object;
static Close close_object;
static ULONG page, ordinal;
struct Region {
    unsigned char *p;
    Region() : p(static_cast<unsigned char *>(VirtualAlloc(nullptr, 8*page, MEM_RESERVE|MEM_COMMIT, PAGE_READWRITE))) {
        if (!p) ExitProcess(2);
        std::memset(p, 0xA5, 8*page);
    }
    ~Region() { if (!VirtualFree(p, 0, MEM_RELEASE)) ExitProcess(3); }
    void protect(unsigned offset, DWORD value) { DWORD old; if (!VirtualProtect(p+offset,page,value,&old)) ExitProcess(4); }
};
static LONG invoke(HANDLE *out, ULONG access, OBJECT_ATTRIBUTES *attrs, ULONG count, ULONG *exception, ULONG *timer_id) {
    LONG status=static_cast<LONG>(0xDEADBEEF); *exception=0;
    __try { status=create_timer(out,timer_id,attrs,count,access); }
    __except(EXCEPTION_EXECUTE_HANDLER) { *exception=GetExceptionCode(); }
    return status;
}
static SIZE_T safe_read(const void *p, void *out, SIZE_T bytes) {
    MEMORY_BASIC_INFORMATION info; SIZE_T got=0;
    if(!VirtualQuery(p,&info,sizeof info)||info.State!=MEM_COMMIT||(info.Protect&(PAGE_NOACCESS|PAGE_GUARD)))return 0;
    auto available=static_cast<const unsigned char *>(info.BaseAddress)+info.RegionSize-static_cast<const unsigned char *>(p);
    if(!ReadProcessMemory(GetCurrentProcess(),p,out,std::min(bytes,static_cast<SIZE_T>(available)),&got))ExitProcess(5);
    return got;
}
static void hex(const char *name,const void *p,unsigned bytes) {
    unsigned char data[128]; auto got=safe_read(p,data,std::min<unsigned>(bytes,sizeof data));
    std::printf(" %s-captured=%zu %s=",name,got,name);
    for(SIZE_T i=0;i<got;++i)std::printf("%02X",data[i]);
}
static ULONG guard(const void *p) { MEMORY_BASIC_INFORMATION info{}; return VirtualQuery(p,&info,sizeof info)?info.Protect&PAGE_GUARD:0; }
static void run(const char *label,ULONG count,ULONG access,unsigned mode=0,unsigned offset=0,ULONG flags=0,ULONG length=sizeof(OBJECT_ATTRIBUTES), unsigned id_mode=0, ULONG id_value=0) {
    Region output, attributes, extra, id_region;
    auto *op=reinterpret_cast<HANDLE *>(output.p+128);
    auto *oa=reinterpret_cast<OBJECT_ATTRIBUTES *>(attributes.p+128);
    OBJECT_ATTRIBUTES value{};value.Length=length;value.Attributes=flags;
    std::memcpy(oa,&value,sizeof value);bool use_attrs=mode>=5;
    switch(mode) {
    case 1:op=nullptr;break;
    case 2:op=reinterpret_cast<HANDLE *>(output.p+128+offset);break;
    case 3:output.protect(0,PAGE_READONLY);break;
    case 4:output.protect(0,PAGE_READWRITE|PAGE_GUARD);break;
    case 5:break;
    case 6:oa=reinterpret_cast<OBJECT_ATTRIBUTES *>(1);break;
    case 7:oa=reinterpret_cast<OBJECT_ATTRIBUTES *>(attributes.p+128+offset);std::memcpy(oa,&value,sizeof value);break;
    case 8:attributes.protect(0,PAGE_NOACCESS);break;
    case 9:attributes.protect(0,PAGE_READWRITE|PAGE_GUARD);break;
    case 10:oa->RootDirectory=reinterpret_cast<HANDLE>(static_cast<uintptr_t>(offset));break;
    case 11:oa->ObjectName=reinterpret_cast<UNICODE_STRING *>(1);break;
    case 12:oa->SecurityDescriptor=reinterpret_cast<void *>(1);break;
    case 13:oa->SecurityQualityOfService=reinterpret_cast<void *>(1);break;
    case 14:op=nullptr;oa=reinterpret_cast<OBJECT_ATTRIBUTES *>(1);break;
    case 15:op=nullptr;break;
    case 16:output.protect(0,PAGE_READONLY);break;
    case 17:output.protect(0,PAGE_READWRITE|PAGE_GUARD);break;
    case 18:attributes.protect(0,PAGE_NOACCESS);op=nullptr;break;
    case 19:attributes.protect(0,PAGE_READWRITE|PAGE_GUARD);op=nullptr;break;
    case 20:op=reinterpret_cast<HANDLE *>(oa);break;
    case 21:op=reinterpret_cast<HANDLE *>(reinterpret_cast<unsigned char *>(oa)+offset);break;
    case 22:oa=reinterpret_cast<OBJECT_ATTRIBUTES *>(attributes.p+page-offset);std::memcpy(oa,&value,sizeof value);attributes.protect(page,PAGE_NOACCESS);break;
    case 23:op=reinterpret_cast<HANDLE *>(output.p+page-offset);output.protect(page,PAGE_NOACCESS);use_attrs=false;break;
    case 24:oa->ObjectName=reinterpret_cast<UNICODE_STRING *>(extra.p+128);std::memset(extra.p+128,0,sizeof(UNICODE_STRING));break;
    case 25:oa->SecurityQualityOfService=extra.p+128;std::memset(extra.p+128,0,16);*reinterpret_cast<ULONG *>(extra.p+128)=12;break;
    case 26:oa->SecurityDescriptor=extra.p+128;std::memset(extra.p+128,0,64);extra.p[128]=1;break;
    case 27:oa=reinterpret_cast<OBJECT_ATTRIBUTES *>(attributes.p+128+offset);std::memcpy(oa,&value,sizeof value);op=nullptr;break;
    }
    if(!use_attrs||mode==28)oa=nullptr;
    auto *timer_id=reinterpret_cast<ULONG *>(id_region.p+128);
    std::memcpy(timer_id,&id_value,sizeof id_value);
    switch(id_mode) {
    case 0:timer_id=nullptr;break;
    case 1:break;
    case 2:timer_id=reinterpret_cast<ULONG *>(1);break;
    case 3:id_region.protect(0,PAGE_READONLY);break;
    case 4:id_region.protect(0,PAGE_READWRITE|PAGE_GUARD);break;
    case 5:timer_id=reinterpret_cast<ULONG *>(id_region.p+128+offset);std::memcpy(timer_id,&id_value,4);break;
    case 6:timer_id=reinterpret_cast<ULONG *>(id_region.p+page-offset);std::memcpy(timer_id,&id_value,4);id_region.protect(page,PAGE_NOACCESS);break;
    case 7:timer_id=reinterpret_cast<ULONG *>(op);break;
    case 8:timer_id=reinterpret_cast<ULONG *>(oa);break;
    }

    auto *backing=mode==20||mode==21?attributes.p+128:output.p+128;
    unsigned repeats=(mode==4||mode==9||mode==17||mode==19||id_mode==4)?2:1;
    for(unsigned repeat=0;repeat<repeats;++repeat) {
        DWORD before=0,after=0;if(!GetProcessHandleCount(GetCurrentProcess(),&before))ExitProcess(6);
        ULONG exception=0;auto status=invoke(op,access,oa,count,&exception,timer_id);
        HANDLE handle=nullptr;auto handle_bytes=op?safe_read(op,&handle,sizeof handle):0;
        unsigned char basic[128]{};unsigned char timer[32];std::memset(timer,0xA5,sizeof timer);ULONG returned=0xA5A5A5A5;
        LONG qs=static_cast<LONG>(0xDEADBEEF),bs=qs,cs=qs;DWORD handle_flags=0;
        if(status>=0 && !exception) {
            if(handle_bytes!=sizeof handle||!handle)ExitProcess(7);
            bs=query_object(handle,0,basic,sizeof(PUBLIC_OBJECT_BASIC_INFORMATION),nullptr);
            qs=query_timer(handle,0,timer,16,&returned);
            if(!GetHandleInformation(handle,&handle_flags))ExitProcess(8);
            cs=close_object(handle);if(cs<0)ExitProcess(9);
        }
        if(!GetProcessHandleCount(GetCurrentProcess(),&after))ExitProcess(10);
        ULONG grant=0;std::memcpy(&grant,basic+4,sizeof grant);
        std::printf("case=%lu label=%s width=%zu count=%08lX access=%08lX mode=%u offset=%u flags=%08lX attrs-length=%lu repeat=%u status=%08lX exception=%08lX handle-bytes=%zu handle=%llX query-object=%08lX grant=%08lX query=%08lX returned=%08lX handle-flags=%lu close=%08lX handles-before=%lu handles-after=%lu output-guard=%lu attrs-guard=%lu",ordinal++,label,sizeof(void *),count,access,mode,offset,flags,length,repeat,static_cast<ULONG>(status),exception,handle_bytes,static_cast<unsigned long long>(reinterpret_cast<uintptr_t>(handle)),static_cast<ULONG>(bs),grant,static_cast<ULONG>(qs),returned,handle_flags,static_cast<ULONG>(cs),before,after,guard(output.p),guard(attributes.p));
        std::printf(" id-mode=%u id-value=%08lX id-guard=%lu timer=",id_mode,id_value,guard(id_region.p));
        for(unsigned i=0;i<sizeof timer;++i)std::printf("%02X",timer[i]);
        hex("id",timer_id,16);hex("id-backing",id_region.p+128,16);
        hex("output",op,96);hex("output-backing",backing,96);hex("attributes",oa,sizeof(OBJECT_ATTRIBUTES));std::puts("");
    }
}
static void type_probe() {
    for(ULONG value:{0ul,2ul,4ul,8ul,0x80000000ul,0x80000008ul}) {
        ULONG timer_id=1;HANDLE handle=nullptr;
        LONG status=create_timer(&handle,value&2?&timer_id:nullptr,nullptr,value,0x1F0003);
        unsigned char data[2048]{};ULONG returned=0;
        LONG query=static_cast<LONG>(0xDEADBEEF),closed=query;
        std::printf("type attributes=%08lX status=%08lX id=%08lX",value,static_cast<ULONG>(status),timer_id);
        if(status>=0){query=query_object(handle,2,data,sizeof data,&returned);if(query>=0){auto name=reinterpret_cast<UNICODE_STRING *>(data);std::printf(" name=%.*ls",name->Length/2,name->Buffer);}std::printf(" wait=%08lX wait-error=%lu",WaitForSingleObject(handle,0),GetLastError());closed=close_object(handle);}
        std::printf(" query=%08lX close=%08lX\n",static_cast<ULONG>(query),static_cast<ULONG>(closed));
    }
}
int main() {
    SYSTEM_INFO info{};GetSystemInfo(&info);page=info.dwPageSize;
    auto module=GetModuleHandleW(L"ntdll.dll");create_timer=reinterpret_cast<Create>(GetProcAddress(module,"NtCreateTimer2"));query_timer=reinterpret_cast<Query>(GetProcAddress(module,"NtQueryTimer"));query_object=reinterpret_cast<Query>(GetProcAddress(module,"NtQueryObject"));close_object=reinterpret_cast<Close>(GetProcAddress(module,"NtClose"));
    if(!create_timer||!query_timer||!query_object||!close_object)return 11;
    HANDLE token=nullptr;if(!OpenProcessToken(GetCurrentProcess(),TOKEN_ADJUST_PRIVILEGES|TOKEN_QUERY,&token))return 20;
    if(!AdjustTokenPrivileges(token,TRUE,nullptr,0,nullptr,nullptr))return 21;
    unsigned char token_data[4096]{};DWORD bytes=0;if(!GetTokenInformation(token,TokenPrivileges,token_data,sizeof token_data,&bytes))return 22;
    auto privileges=reinterpret_cast<TOKEN_PRIVILEGES *>(token_data);unsigned enabled=0;
    for(DWORD i=0;i<privileges->PrivilegeCount;++i)if(privileges->Privileges[i].Attributes&SE_PRIVILEGE_ENABLED)++enabled;
    if(!CloseHandle(token)||enabled)return 23;
    std::printf("profile width=%zu page=%lu processors=%lu attrs-size=%zu privileges-enabled=%u\n",sizeof(void *),page,info.dwNumberOfProcessors,sizeof(OBJECT_ATTRIBUTES),enabled);
    for(ULONG id=0;id<=64;++id)for(ULONG flags:{2ul,6ul,0x80000002ul})
        run("reduced-id-range",flags,0x1F0003,28,0,0,sizeof(OBJECT_ATTRIBUTES),1,id);
    for(ULONG id:{0x7FFFFFFFul,0x80000000ul,0xFFFFFFFFul})for(ULONG flags:{2ul,6ul,0x80000002ul})
        run("reduced-id-width",flags,0x1F0003,28,0,0,sizeof(OBJECT_ATTRIBUTES),1,id);
    for(ULONG flags:{0ul,2ul,4ul,6ul,8ul,12ul,0x80000000ul,0x80000002ul,0x8000000Cul})
        for(ULONG access:{0ul,1ul,2ul,0x100002ul,0x1F0003ul,0x01000000ul,0xFFFFFFFFul})
            run("reduced-access",flags,access,28,0,0,sizeof(OBJECT_ATTRIBUTES),flags&2?1:0,1);
    type_probe();
    std::printf("complete cases=%lu\n",ordinal);
    return 0;
}
