#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <winternl.h>
#include <cstdio>
#include <cstdint>
#include <cstring>
#include <initializer_list>
using Open=LONG(NTAPI*)(HANDLE*,ULONG,OBJECT_ATTRIBUTES*); using Close=LONG(NTAPI*)(HANDLE);
static Open open_nt; static Close close_nt; static unsigned count;
static void fail(const char* what){printf("failure=%s error=%lu\n",what,GetLastError());ExitProcess(21);}
static DWORD prot(void* p){MEMORY_BASIC_INFORMATION m{};if(!VirtualQuery(p,&m,sizeof(m)))fail("query");return m.Protect;}
static void guard(void* p){DWORD old;if(!VirtualProtect(p,4096,PAGE_READWRITE|PAGE_GUARD,&old))fail("guard");}
static LONG call(HANDLE* out,OBJECT_ATTRIBUTES* a,ULONG* exception){__try{return open_nt(out,3,a);}__except(EXCEPTION_EXECUTE_HANDLER){*exception=GetExceptionCode();return LONG(0xDEADBEEF);}}
static void one(unsigned pattern,USHORT length,ULONG flags,unsigned root,unsigned output,unsigned layout){
 DWORD before=0,after=0;GetProcessHandleCount(GetCurrentProcess(),&before);
 auto m=static_cast<unsigned char*>(VirtualAlloc(nullptr,20*4096,MEM_RESERVE|MEM_COMMIT,PAGE_READWRITE));if(!m)fail("allocate");memset(m,0xA5,20*4096);
 const unsigned prefix[]={0,2,4,6,8,10,14,16,18,20,24,26,32};auto buffer=m+(layout?4096-prefix[layout]:0);
 for(unsigned i=0;i<65534;i+=2){buffer[i]='X';buffer[i+1]=0;}
 const wchar_t* names[]={L"\\KnownDlls",L"\\KnownDlls32",L"\\Absent",L"KnownDlls"};memcpy(buffer,names[pattern],wcslen(names[pattern])*2);
 UNICODE_STRING u{};u.Length=length;u.MaximumLength=length;u.Buffer=reinterpret_cast<PWSTR>(buffer);
 OBJECT_ATTRIBUTES a{};a.Length=sizeof(a);a.RootDirectory=root?reinterpret_cast<HANDLE>(~uintptr_t(0)):nullptr;a.ObjectName=&u;a.Attributes=flags;
 auto outp=output==0?nullptr:output==1?reinterpret_cast<HANDLE*>(1):reinterpret_cast<HANDLE*>(m+18*4096);
 guard(layout?m+4096:m);if(output==2)guard(m+18*4096);
 ULONG exception=0;LONG s=call(outp,&a,&exception);DWORD bp=prot(layout?m+4096:m),op=prot(m+18*4096);
 DWORD old;if(!VirtualProtect(m,20*4096,PAGE_READWRITE,&old))fail("unprotect");uintptr_t h=0;if(output>=2)memcpy(&h,outp,sizeof(h));
 if(s>=0&&exception==0&&h&&close_nt(reinterpret_cast<HANDLE>(h))<0)fail("close");if(!VirtualFree(m,0,MEM_RELEASE))fail("free");GetProcessHandleCount(GetCurrentProcess(),&after);
 printf("case=%u width=%zu pattern=%u length=%u flags=%08lX root=%u output=%u layout=%u prefix=%u status=%08lX exception=%08lX buffer-protect=%08lX output-protect=%08lX output-after=%llX handles-before=%lu handles-after=%lu\n",count++,sizeof(void*),pattern,length,flags,root,output,layout,prefix[layout],ULONG(s),exception,bp,op,(unsigned long long)h,before,after);if(before!=after)fail("lifetime");
}
int main(){setvbuf(stdout,nullptr,_IONBF,0);auto n=GetModuleHandleW(L"ntdll.dll");open_nt=reinterpret_cast<Open>(GetProcAddress(n,"NtOpenDirectoryObject"));close_nt=reinterpret_cast<Close>(GetProcAddress(n,"NtClose"));if(!open_nt||!close_nt)fail("exports");auto get_session=reinterpret_cast<ULONG(NTAPI*)()>(GetProcAddress(n,"RtlGetCurrentServiceSessionId"));if(!get_session)fail("session-export");auto get_root=reinterpret_cast<PCWSTR(NTAPI*)()>(GetProcAddress(n,"RtlGetNtSystemRoot"));if(!get_root)fail("root-export");auto system_root=get_root();size_t root_length=wcsnlen(system_root,260);if(root_length==260)fail("root-bounds");printf("subject session=%lu root-data=",get_session());for(size_t i=0;i<root_length;i++)printf("%02X%02X",unsigned(system_root[i]&255),unsigned(system_root[i]>>8));printf("\n");
 for(ULONG flags:{0u,0x40u})for(unsigned root=0;root<2;root++)for(unsigned output=0;output<4;output++)for(unsigned pattern=0;pattern<4;pattern++)for(USHORT length:{USHORT(0),USHORT(1),USHORT(2),USHORT(18),USHORT(20),USHORT(22),USHORT(64),USHORT(65532)})for(unsigned layout=0;layout<13;layout++)one(pattern,length,flags,root,output,layout);
 printf("complete cases=%u\n",count);return 0;
}
