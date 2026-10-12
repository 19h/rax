#include <windows.h>
#include <winternl.h>
#include <cstdio>
#include <cstring>
#include <initializer_list>
using Create=LONG(NTAPI*)(PHANDLE,ACCESS_MASK,POBJECT_ATTRIBUTES);
using Port=LONG(NTAPI*)(PHANDLE,ACCESS_MASK,POBJECT_ATTRIBUTES,ULONG);
using Event=LONG(NTAPI*)(PHANDLE,ACCESS_MASK,POBJECT_ATTRIBUTES,ULONG,BOOLEAN);
using Timer=LONG(NTAPI*)(PHANDLE,PULONG,POBJECT_ATTRIBUTES,ULONG,ACCESS_MASK);
using Associate=LONG(NTAPI*)(HANDLE,HANDLE,HANDLE,PVOID,PVOID,LONG,ULONG_PTR,PBOOLEAN);
using Cancel=LONG(NTAPI*)(HANDLE,BOOLEAN);
using Remove=LONG(NTAPI*)(HANDLE,PVOID*,PVOID*,PIO_STATUS_BLOCK,PLARGE_INTEGER);
using Set=LONG(NTAPI*)(HANDLE,PLONG);
using Close=LONG(NTAPI*)(HANDLE);
static Create packet;static Port port;static Event event;static Timer timer;
static Associate associate;static Cancel cancel;static Remove remove_packet;static Set set_event;static Close close_object;
static unsigned ordinal;
static DWORD wait(HANDLE h){return WaitForSingleObject(h,0);}
static void run(unsigned kind,BOOLEAN signaled,int cancel_mode){
 DWORD before=0,after=0;GetProcessHandleCount(GetCurrentProcess(),&before);
 HANDLE w=nullptr,p=nullptr,t=nullptr;
 LONG a=packet(&w,0x1F0001,nullptr),b=port(&p,0x1F0003,nullptr,1);
 LONG c=kind==2?timer(&t,nullptr,nullptr,8,0x1F0003):event(&t,0x1F0003,nullptr,kind,signaled);
 if(a<0||b<0||c<0)ExitProcess(41);
 unsigned char already[16];std::memset(already,0xA5,sizeof already);
 DWORD initial=wait(w);
 LONG status=associate(w,p,t,reinterpret_cast<void*>(ULONG_PTR(0x1234567887654321ull)),reinterpret_cast<void*>(ULONG_PTR(0xA1B2C3D4CCCCDDDDull)),LONG(0xC1234567),ULONG_PTR(0xFEDCBA9876543210ull),already);
 DWORD associated=wait(w);LONG cancel_status=LONG(0xDEADBEEF);
 if(cancel_mode>=0)cancel_status=cancel(w,BOOLEAN(cancel_mode));
 DWORD after_cancel=wait(w);LONG set_status=LONG(0xDEADBEEF);
 if(kind!=2)set_status=set_event(t,nullptr);
 DWORD after_signal=wait(w);
 PVOID key=reinterpret_cast<void*>(ULONG_PTR(0xA5A5A5A5A5A5A5A5ull)),apc=key;
 IO_STATUS_BLOCK io;std::memset(&io,0xA5,sizeof io);LARGE_INTEGER zero{};
 LONG removed=remove_packet(p,&key,&apc,&io,&zero);DWORD after_remove=wait(w);
 LONG canceled_again=cancel(w,TRUE);DWORD final_wait=wait(w);
 LONG close_t=close_object(t),close_p=close_object(p),close_w=close_object(w);
 GetProcessHandleCount(GetCurrentProcess(),&after);
 std::printf("case=%u width=%zu kind=%u signaled=%u cancel-mode=%d status=%08lX initial=%08lX associated=%08lX cancel=%08lX after-cancel=%08lX set=%08lX after-signal=%08lX removed=%08lX key=%llX apc=%llX io-status=%08lX information=%llX after-remove=%08lX cancel-again=%08lX final-wait=%08lX close-target=%08lX close-port=%08lX close-packet=%08lX handles-before=%lu handles-after=%lu already=",ordinal++,sizeof(void*),kind,signaled,cancel_mode,ULONG(status),initial,associated,ULONG(cancel_status),after_cancel,ULONG(set_status),after_signal,ULONG(removed),(unsigned long long)ULONG_PTR(key),(unsigned long long)ULONG_PTR(apc),ULONG(io.Status),(unsigned long long)io.Information,after_remove,ULONG(canceled_again),final_wait,ULONG(close_t),ULONG(close_p),ULONG(close_w),before,after);
 for(unsigned char byte:already)std::printf("%02X",unsigned(byte));std::printf("\n");
 if(close_t<0||close_p<0||close_w<0||before!=after)ExitProcess(42);
}
int main(){
 auto n=GetModuleHandleW(L"ntdll.dll");
#define LOAD(variable,type,name) variable=reinterpret_cast<type>(GetProcAddress(n,name));if(!variable)return 43
 LOAD(packet,Create,"NtCreateWaitCompletionPacket");LOAD(port,Port,"NtCreateIoCompletion");LOAD(event,Event,"NtCreateEvent");LOAD(timer,Timer,"NtCreateTimer2");LOAD(associate,Associate,"NtAssociateWaitCompletionPacket");LOAD(cancel,Cancel,"NtCancelWaitCompletionPacket");LOAD(remove_packet,Remove,"NtRemoveIoCompletion");LOAD(set_event,Set,"NtSetEvent");LOAD(close_object,Close,"NtClose");
 for(unsigned kind:{0u,1u,2u})for(BOOLEAN signal:{BOOLEAN(0),BOOLEAN(1)}){
  if(kind==2&&signal)continue;
  for(int mode:{-1,0,1})run(kind,signal,mode);
 }
 std::printf("complete cases=%u\n",ordinal);return 0;
}
