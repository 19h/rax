#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <winternl.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>
#include <initializer_list>
using Open=LONG(NTAPI*)(HANDLE*,ULONG,OBJECT_ATTRIBUTES*);
using Query=LONG(NTAPI*)(HANDLE,ULONG,void*,ULONG,ULONG*);
using Close=LONG(NTAPI*)(HANDLE);
using Wait=LONG(NTAPI*)(HANDLE,BOOLEAN,LARGE_INTEGER*);
static Open open_nt;static Query query;static Close close_nt;static Wait wait_nt;
static unsigned count=0;
static void fail(const char* s){printf("failure=%s error=%lu\n",s,GetLastError());ExitProcess(21);}
static void dump(const unsigned char* b,size_t n){for(size_t i=0;i<n;i++)printf("%02X",unsigned(b[i]));}
static LONG call(HANDLE* out,ULONG access,OBJECT_ATTRIBUTES* attrs,ULONG* exception){
 __try{return open_nt(out,access,attrs);}__except(EXCEPTION_EXECUTE_HANDLER){*exception=GetExceptionCode();return LONG(0xDEADBEEF);}
}
static HANDLE base(const wchar_t* path){UNICODE_STRING name{};name.Buffer=const_cast<PWSTR>(path);name.Length=USHORT(wcslen(path)*2);name.MaximumLength=name.Length+2;OBJECT_ATTRIBUTES attrs{};attrs.Length=sizeof(attrs);attrs.ObjectName=&name;attrs.Attributes=0x40;HANDLE h=nullptr;ULONG exception=0;LONG status=call(&h,3,&attrs,&exception);if(status<0||exception||!h)fail("base-directory");return h;}
static void release(HANDLE h){if(!h)return;SetHandleInformation(h,HANDLE_FLAG_PROTECT_FROM_CLOSE,0);if(close_nt(h)<0)fail("release");}
static void one(unsigned family,unsigned name_id,ULONG access,ULONG flags,ULONG attribute_length,unsigned root_kind,unsigned mode,ULONG supplied_length,ULONG maximum){
 static const wchar_t* names[]={L"\\KnownDlls",L"\\KnownDlls32",L"\\knowndlls",L"KnownDlls",L"\\",L"",L"\\KnownDlls\\",L"\\KnownDlls\\\\x",L"\\KnownDlls\\rax-absent-20261012",L"\\rax-absent-20261012\\x",L"\\KnownDlls\\KnownDllPath",L"\\KnownDlls\\ntdll.dll",L"KnownDllPath",L"ntdll.dll"};
 DWORD hb=0,ha=0;if(!GetProcessHandleCount(GetCurrentProcess(),&hb))fail("count-before");
 std::vector<wchar_t> storage(32768,0);size_t natural=wcslen(names[name_id])*2;memcpy(storage.data(),names[name_id],natural);UNICODE_STRING name{};name.Buffer=storage.data();name.Length=USHORT(supplied_length==0xFFFFFFFF?natural:supplied_length);name.MaximumLength=USHORT(maximum==0xFFFFFFFF?name.Length+2:maximum);
 const USHORT selected_length=name.Length,selected_maximum=name.MaximumLength;
 HANDLE root=nullptr,event=nullptr;uintptr_t selected=0;
 if(root_kind==1||root_kind==3){root=base(L"\\KnownDlls");selected=uintptr_t(root);if(root_kind==3){release(root);root=nullptr;}}
 else if(root_kind==2){event=CreateEventW(nullptr,FALSE,FALSE,nullptr);if(!event)fail("event-root");selected=uintptr_t(event);}
 else if(root_kind==4)selected=uintptr_t(-1);else if(root_kind==5)selected=uintptr_t(-2);else if(root_kind==6)selected=1;
 OBJECT_ATTRIBUTES attrs{};attrs.Length=attribute_length;attrs.RootDirectory=reinterpret_cast<HANDLE>(selected);attrs.ObjectName=&name;attrs.Attributes=flags;
 unsigned char original[sizeof(attrs)],name_original[sizeof(name)];memcpy(original,&attrs,sizeof(attrs));memcpy(name_original,&name,sizeof(name));
 uintptr_t out[2]={uintptr_t(0xA5A5A5A5A5A5A5A5ull),uintptr_t(0xB6B6B6B6B6B6B6B6ull)};
 auto outp=reinterpret_cast<HANDLE*>(out);OBJECT_ATTRIBUTES* ap=&attrs;
 if(mode==1)ap=nullptr;else if(mode==2)attrs.ObjectName=nullptr;else if(mode==3)name.Buffer=nullptr;else if(mode==4)outp=nullptr;else if(mode==5)outp=reinterpret_cast<HANDLE*>(&attrs);else if(mode==6)outp=reinterpret_cast<HANDLE*>(&name);
 // Retain the actual pre-call bytes, after selecting each pointer mode.
 memcpy(original,&attrs,sizeof(attrs));memcpy(name_original,&name,sizeof(name));
 ULONG exception=0;LONG status=call(outp,access,ap,&exception);HANDLE actual=nullptr;if(status>=0&&exception==0&&outp)memcpy(&actual,outp,sizeof(actual));
 LONG basic_status=LONG(0xDEADBEEF),type_status=LONG(0xDEADBEEF),name_status=LONG(0xDEADBEEF),wait_status=LONG(0xDEADBEEF),closed=LONG(0xDEADBEEF);ULONG basic_bytes=0,type_bytes=0,name_bytes=0;unsigned char basic[56],type[256],object_name[512];memset(basic,0xA5,sizeof(basic));memset(type,0xA5,sizeof(type));memset(object_name,0xA5,sizeof(object_name));DWORD handle_flags=0;
 if(actual){basic_status=query(actual,0,basic,sizeof(basic),&basic_bytes);type_status=query(actual,2,type,sizeof(type),&type_bytes);name_status=query(actual,1,object_name,sizeof(object_name),&name_bytes);GetHandleInformation(actual,&handle_flags);LARGE_INTEGER timeout{};wait_status=wait_nt(actual,FALSE,&timeout);SetHandleInformation(actual,HANDLE_FLAG_PROTECT_FROM_CLOSE,0);closed=close_nt(actual);if(closed<0)fail("close-result");}
 release(root);if(event)CloseHandle(event);if(!GetProcessHandleCount(GetCurrentProcess(),&ha))fail("count-after");
 printf("case=%u width=%zu family=%u name=%u access=%08lX flags=%08lX attributes-length=%lu root=%u mode=%u name-length=%u maximum=%u status=%08lX exception=%08lX output=%llX canary=%llX actual=%llX basic=%08lX basic-bytes=%lu type=%08lX type-bytes=%lu object-name=%08lX object-name-bytes=%lu wait=%08lX handle-flags=%lu close=%08lX handles-before=%lu handles-after=%lu attributes-before=",count++,sizeof(void*),family,name_id,access,flags,attribute_length,root_kind,mode,selected_length,selected_maximum,ULONG(status),exception,(unsigned long long)out[0],(unsigned long long)out[1],(unsigned long long)uintptr_t(actual),ULONG(basic_status),basic_bytes,ULONG(type_status),type_bytes,ULONG(name_status),name_bytes,ULONG(wait_status),handle_flags,ULONG(closed),hb,ha);dump(original,sizeof(original));printf(" attributes-after=");dump(reinterpret_cast<unsigned char*>(&attrs),sizeof(attrs));printf(" unicode-before=");dump(name_original,sizeof(name_original));printf(" unicode-after=");dump(reinterpret_cast<unsigned char*>(&name),sizeof(name));printf(" basic-data=");dump(basic,sizeof(basic));printf(" type-data=");dump(type,type_bytes<=sizeof(type)?type_bytes:0);printf(" name-data=");dump(object_name,name_bytes<=sizeof(object_name)?name_bytes:0);puts("");if(hb!=ha)fail("lifetime");
}
int main(){setvbuf(stdout,nullptr,_IONBF,0);auto n=GetModuleHandleW(L"ntdll.dll");open_nt=reinterpret_cast<Open>(GetProcAddress(n,"NtOpenDirectoryObject"));query=reinterpret_cast<Query>(GetProcAddress(n,"NtQueryObject"));close_nt=reinterpret_cast<Close>(GetProcAddress(n,"NtClose"));wait_nt=reinterpret_cast<Wait>(GetProcAddress(n,"NtWaitForSingleObject"));if(!open_nt||!query||!close_nt||!wait_nt)fail("exports");
 for(unsigned name=0;name<14;name++)for(ULONG access:{0u,1u,2u,3u,4u,8u,15u,0x10000u,0x20000u,0x40000u,0x80000u,0xF0000u,0x100000u,0x01000000u,0x02000000u,0x80000000u,0x40000000u,0x20000000u,0x10000000u,0xFFFFFFFFu})for(ULONG flags:{0u,0x40u})one(0,name,access,flags,sizeof(OBJECT_ATTRIBUTES),0,0,0xFFFFFFFF,0xFFFFFFFF);
 for(ULONG flags:{0u,1u,2u,4u,8u,0x10u,0x20u,0x40u,0x80u,0x100u,0x200u,0x400u,0x800u,0x1000u,0x2000u,0xFFFFFFFFu})for(unsigned name:{0u,2u,8u})one(1,name,3,flags,sizeof(OBJECT_ATTRIBUTES),0,0,0xFFFFFFFF,0xFFFFFFFF);
 for(ULONG length:{0u,1u,4u,23u,24u,25u,47u,48u,49u,0xFFFFFFFFu})for(unsigned mode=0;mode<7;mode++)one(2,0,3,0x40,length,0,mode,0xFFFFFFFF,0xFFFFFFFF);
 for(unsigned root=0;root<7;root++)for(unsigned name:{0u,3u,4u,5u,8u,12u,13u})one(3,name,3,0x40,sizeof(OBJECT_ATTRIBUTES),root,0,0xFFFFFFFF,0xFFFFFFFF);
 for(ULONG length:{0u,1u,2u,19u,20u,21u,22u,24u,65534u,65535u})for(ULONG maximum:{0u,1u,19u,20u,21u,22u,65535u,0xFFFFFFFFu})for(unsigned mode:{0u,2u,3u})one(4,0,3,0x40,sizeof(OBJECT_ATTRIBUTES),0,mode,length,maximum);
 printf("complete cases=%u\n",count);return 0;
}
