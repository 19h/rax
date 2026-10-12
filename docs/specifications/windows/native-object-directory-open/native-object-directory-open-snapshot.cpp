#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <winternl.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <initializer_list>
using Open=LONG(NTAPI*)(HANDLE*,ULONG,OBJECT_ATTRIBUTES*);using QueryObject=LONG(NTAPI*)(HANDLE,ULONG,void*,ULONG,ULONG*);using QueryDirectory=LONG(NTAPI*)(HANDLE,void*,ULONG,BOOLEAN,BOOLEAN,ULONG*,ULONG*);using QueryLink=LONG(NTAPI*)(HANDLE,UNICODE_STRING*,ULONG*);using Close=LONG(NTAPI*)(HANDLE);
static Open open_directory,open_link;static QueryObject query_object;static QueryDirectory query_directory;static QueryLink query_link;static Close close_nt;
struct Record{UNICODE_STRING name,type;};
static void fail(const char* what){printf("failure=%s error=%lu\n",what,GetLastError());ExitProcess(21);}
static void dump(const void* p,size_t n){auto b=static_cast<const unsigned char*>(p);for(size_t i=0;i<n;i++)printf("%02X",unsigned(b[i]));}
static LONG open(Open operation,const wchar_t* path,HANDLE root,ULONG access,HANDLE* out){UNICODE_STRING name{};name.Buffer=const_cast<PWSTR>(path);name.Length=USHORT(wcslen(path)*2);name.MaximumLength=name.Length+2;OBJECT_ATTRIBUTES attrs{};attrs.Length=sizeof(attrs);attrs.RootDirectory=root;attrs.ObjectName=&name;attrs.Attributes=0x40;return operation(out,access,&attrs);}
static bool inside(const unsigned char* b,size_t n,const UNICODE_STRING& u){auto p=reinterpret_cast<uintptr_t>(u.Buffer),at=reinterpret_cast<uintptr_t>(b);return u.Length%2==0&&p>=at&&p-at<=n&&u.Length<=n-(p-at);}
int main(){setvbuf(stdout,nullptr,_IONBF,0);auto n=GetModuleHandleW(L"ntdll.dll");open_directory=reinterpret_cast<Open>(GetProcAddress(n,"NtOpenDirectoryObject"));open_link=reinterpret_cast<Open>(GetProcAddress(n,"NtOpenSymbolicLinkObject"));query_object=reinterpret_cast<QueryObject>(GetProcAddress(n,"NtQueryObject"));query_directory=reinterpret_cast<QueryDirectory>(GetProcAddress(n,"NtQueryDirectoryObject"));query_link=reinterpret_cast<QueryLink>(GetProcAddress(n,"NtQuerySymbolicLinkObject"));close_nt=reinterpret_cast<Close>(GetProcAddress(n,"NtClose"));if(!open_directory||!open_link||!query_object||!query_directory||!query_link||!close_nt)fail("exports");
 DWORD before=0,after=0;GetProcessHandleCount(GetCurrentProcess(),&before);unsigned directories=0,entries=0,links=0;
 for(auto path:{L"\\",L"\\KnownDlls",L"\\KnownDlls32"}){
  HANDLE h=nullptr;LONG status=open(open_directory,path,nullptr,0x02000000,&h);printf("directory ordinal=%u width=%zu path=",directories,sizeof(void*));dump(path,wcslen(path)*2);printf(" status=%08lX",ULONG(status));if(status<0){puts("");continue;}if(!h)fail("null-directory");
  unsigned char basic[56]{};ULONG size=0;LONG bs=query_object(h,0,basic,sizeof(basic),&size);printf(" basic=%08lX basic-bytes=%lu data=",ULONG(bs),size);dump(basic,sizeof(basic));puts("");if(bs<0||size!=56)fail("basic");
  ULONG context=0;unsigned ordinal=0;
  for(;;){alignas(void*) unsigned char buffer[4096]{};ULONG returned=0;LONG result=query_directory(h,buffer,sizeof(buffer),TRUE,FALSE,&context,&returned);if(result==LONG(0x8000001A)){printf("directory-complete ordinal=%u entries=%u context=%lu terminal=%08lX\n",directories,ordinal,context,ULONG(result));break;}if(result<0||returned>sizeof(buffer)||returned<sizeof(Record))fail("enumeration");auto rec=reinterpret_cast<Record*>(buffer);if(!inside(buffer,sizeof(buffer),rec->name)||!inside(buffer,sizeof(buffer),rec->type))fail("enumeration-string");
   printf("entry directory=%u ordinal=%u context=%lu name=",directories,ordinal++,context);dump(rec->name.Buffer,rec->name.Length);printf(" type=");dump(rec->type.Buffer,rec->type.Length);puts("");entries++;
   std::wstring type(rec->type.Buffer,rec->type.Length/2),name(rec->name.Buffer,rec->name.Length/2);
   if(directories!=0&&type==L"SymbolicLink"){
    HANDLE link=nullptr;LONG ls=open(open_link,name.c_str(),h,1,&link);printf("link directory=%u name=",directories);dump(name.data(),name.size()*2);printf(" open=%08lX",ULONG(ls));if(ls<0){puts("");continue;}wchar_t value[2048]{};UNICODE_STRING out{};out.Buffer=value;out.MaximumLength=sizeof(value);ULONG required=0;LONG qs=query_link(link,&out,&required);printf(" query=%08lX length=%u required=%lu target=",ULONG(qs),out.Length,required);if(out.Length>sizeof(value))fail("link-bound");dump(value,out.Length);LONG closed=close_nt(link);printf(" close=%08lX\n",ULONG(closed));if(qs<0||closed<0)fail("link-query");links++;
   }
   if(ordinal>8192)fail("entry-bound");
  }
  if(close_nt(h)<0)fail("close-directory");directories++;
 }
 GetProcessHandleCount(GetCurrentProcess(),&after);printf("complete directories=%u entries=%u links=%u handles-before=%lu handles-after=%lu\n",directories,entries,links,before,after);if(before!=after||directories!=3)return 22;return 0;
}
