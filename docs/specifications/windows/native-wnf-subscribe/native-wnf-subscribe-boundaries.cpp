#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <vector>
#include <utility>
using Subscribe=LONG(NTAPI *)(const uint64_t *,ULONG,ULONG,uint64_t *);
using Unsubscribe=LONG(NTAPI *)(const uint64_t *);
using Info=LONG(NTAPI *)(const uint64_t *,ULONG,void *,void *,ULONG);
constexpr uint64_t known=0x41911A28A3BC10B5,poison=0xA5A5A5A5A5A5A5A5;
int main(){
 setvbuf(stdout,nullptr,_IONBF,0);auto m=GetModuleHandleW(L"ntdll.dll");
 auto sub=reinterpret_cast<Subscribe>(GetProcAddress(m,"NtSubscribeWnfStateChange"));
 auto unsub=reinterpret_cast<Unsubscribe>(GetProcAddress(m,"NtUnsubscribeWnfStateChange"));
 auto info=reinterpret_cast<Info>(GetProcAddress(m,"NtQueryWnfStateNameInformation"));
 if(!sub||!unsub||!info)return 2;
 unsigned ordinal=0;std::vector<std::pair<unsigned,uint64_t>> names{{64,known},{65,0},{66,~uint64_t(0)}};
 for(unsigned bit=0;bit<64;bit++)names.emplace_back(bit,known^(uint64_t(1)<<bit));
 for(auto [bit,name]:names)for(ULONG mask=0;mask<64;mask++){
  unsub(&name);uint64_t id=poison;LONG status=sub(&name,0,mask,&id);ULONG present=0xA5A5A5A5;
  LONG q=info(&name,1,nullptr,&present,4),end=unsub(&name);
  printf("name-case=%u width=%zu bit=%u name=%016llX mask=%08lX status=%08lX id=%016llX query=%08lX present=%08lX end=%08lX\n",ordinal++,sizeof(void *),bit,(unsigned long long)name,mask,ULONG(status),(unsigned long long)id,ULONG(q),present,ULONG(end));
 }
 SYSTEM_INFO si{};GetSystemInfo(&si);SIZE_T page=si.dwPageSize;auto memory=static_cast<unsigned char *>(VirtualAlloc(nullptr,page*4,MEM_RESERVE|MEM_COMMIT,PAGE_READWRITE));if(!memory)return 3;
 DWORD old=0;if(!VirtualProtect(memory+page,page,PAGE_NOACCESS,&old)||!VirtualProtect(memory+page*3,page,PAGE_NOACCESS,&old))return 4;
 std::vector<std::pair<unsigned,const uint64_t *>> inputs{{0,&known},{1,nullptr},{2,reinterpret_cast<const uint64_t *>(1)},{3,reinterpret_cast<const uint64_t *>(memory+page)},{4,reinterpret_cast<const uint64_t *>(memory+page-4)}};
 for(unsigned offset=0;offset<8;offset++){memcpy(memory+32+16*offset+offset,&known,8);inputs.emplace_back(5+offset,reinterpret_cast<const uint64_t *>(memory+32+16*offset+offset));}
 memcpy(memory+page-4,&known,4);
 std::vector<std::pair<unsigned,uint64_t *>> outputs{{0,reinterpret_cast<uint64_t *>(memory+page*2+32)},{1,nullptr},{2,reinterpret_cast<uint64_t *>(1)},{3,reinterpret_cast<uint64_t *>(memory+page*3)},{4,reinterpret_cast<uint64_t *>(memory+page*3-4)}};
 for(unsigned offset=1;offset<8;offset++)outputs.emplace_back(4+offset,reinterpret_cast<uint64_t *>(memory+page*2+64+16*offset+offset));
 for(auto [input_kind,input]:inputs)for(auto [output_kind,output]:outputs)for(ULONG mask:{0u,0x11u,0x20u,0xFFFFFFFFu}){
  unsub(&known);uint64_t before=poison,after=poison;
  if(output_kind==0||output_kind>=5)memcpy(output,&poison,8);
  if(output_kind==4)memcpy(output,&poison,4);
  LONG status=sub(input,0,mask,output);if(output_kind==0||output_kind>=5)memcpy(&after,output,8);if(output_kind==4)memcpy(&after,output,4);
  uint64_t repeat_id=poison;LONG repeat=sub(&known,0,0x11,&repeat_id),end=unsub(&known);
  printf("pointer-case=%u width=%zu input=%u output=%u mask=%08lX status=%08lX before=%016llX after=%016llX repeat=%08lX repeat-id=%016llX end=%08lX\n",ordinal++,sizeof(void *),input_kind,output_kind,mask,ULONG(status),(unsigned long long)before,(unsigned long long)after,ULONG(repeat),(unsigned long long)repeat_id,ULONG(end));
 }
 // Repeated updates must preserve identity and expose the current requested mask.
 for(ULONG first:{0u,1u,2u,4u,8u,0x10u,0x11u,0x1Fu})for(ULONG second:{0u,1u,2u,4u,8u,0x10u,0x11u,0x1Fu}){
  unsub(&known);uint64_t id=poison,id2=poison;LONG a=sub(&known,0,first,&id),b=sub(&known,~ULONG(0),second,&id2);ULONG present=0xA5A5A5A5;LONG q=info(&known,1,nullptr,&present,4),end=unsub(&known),twice=unsub(&known);
  printf("sequence-case=%u width=%zu first=%08lX second=%08lX status=%08lX id=%016llX update=%08lX update-id=%016llX query=%08lX present=%08lX end=%08lX twice=%08lX\n",ordinal++,sizeof(void *),first,second,ULONG(a),(unsigned long long)id,ULONG(b),(unsigned long long)id2,ULONG(q),present,ULONG(end),ULONG(twice));
 }
 VirtualFree(memory,0,MEM_RELEASE);printf("complete cases=%u\n",ordinal);return 0;
}
