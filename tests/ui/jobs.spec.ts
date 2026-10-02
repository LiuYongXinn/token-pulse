import { installSyntheticCalendar } from './calendar-bridge';
import { test, expect } from '@playwright/test';

test('synthetic job IPC fixture shows accepted cancellation until final state and retains request idempotency', async ({ page }) => {
  await installSyntheticCalendar(page);
  await page.addInitScript(() => {
    let jobs: Record<string, unknown>[] = [];let requestKey:string|null=null;let lostResponse=true;
    Object.assign(window, {isTauri:true,__TAURI_EVENT_PLUGIN_INTERNALS__:{unregisterListener:()=>{}},__TAURI_INTERNALS__:{transformCallback:()=>0,invoke:async(command:string,args:Record<string,unknown>)=>{
      if (command === 'plugin:event|listen' || command === 'plugin:event|unlisten') return 0;
      const response=(data:unknown)=>({api_version:1,request_id:args.requestId,display_policy:{settings_revision:'1',privacy:false},data});
      if (command === 'get_display_settings' || command === 'resolve_calendar_selection') return response(window.__syntheticCalendar(command, args));
      if(command==='get_app_status')return response({version:'synthetic-test',development:true,data_directory:'synthetic-test',collector:'ready',storage:'ready',storage_error:null,quota:'not_configured',taskbar:'not_implemented'});
      if(command==='list_jobs')return response(structuredClone(jobs));
      if(command==='get_taskbar_status')return response({revision:'0',state:'disabled',applied_settings_revision:null,issue:null,error:null,compact:null,fallback_visible:null,fallback_error:null,last_cleanup:null,last_snapshot_at_ms:null});
      if(command==='start_job') {
        const request=args.request as {request_key:string};
        if(requestKey!==null && requestKey!==request.request_key)throw new Error('lost idempotency key');
        requestKey=request.request_key;
        if(jobs.length===0)jobs=[{job_id:'synthetic-job',kind:'rebuild',state:'running',phase:'replaying_observations',discovered_files:'2',discovery_complete:true,processed_files:'1',processed_bytes:'92233720368547758080',accepted_events:'25',pending_observations:'1',can_cancel:true,error:null,created_at_ms:1000,updated_at_ms:2000}];
        if(lostResponse){lostResponse=false;throw new Error('synthetic lost reply');}return response(structuredClone(jobs[0]));
      }
      if(command==='cancel_job') {jobs[0].state='cancelling';return response('accepted');}
      throw new Error(`unexpected synthetic command ${command}`);
    }},__setSyntheticJobState:(state:string)=>{jobs[0].state=state;jobs[0].can_cancel=false;}});
  });
  await page.goto('/');await page.getByRole('button',{name:'采集诊断',exact:true}).click();
  await expect(page.getByRole('button',{name:'重建全部账本'})).toBeEnabled();
  await page.getByRole('button',{name:'重建全部账本'}).click();await expect(page.getByText('synthetic lost reply')).toBeVisible();
  await page.getByRole('button',{name:'重建全部账本'}).click();await expect(page.getByText('重建已提交，正在执行。')).toBeVisible();
  await expect(page.getByText('已处理 92,233,720,368,547,758,080 字节')).toBeVisible();
  await page.getByRole('button',{name:'取消作业',exact:true}).click();await expect(page.getByText('已请求安全取消，等待当前批次结束。')).toBeVisible();await expect(page.getByText('正在安全取消')).toBeVisible();await expect(page.getByRole('button',{name:'等待安全取消'})).toBeDisabled();
  await page.evaluate(()=>{(window as unknown as {__setSyntheticJobState:(s:string)=>void}).__setSyntheticJobState('cancelled');});
  await page.getByRole('button',{name:'刷新作业'}).click();await expect(page.getByText('已取消',{exact:true})).toBeVisible();await expect(page.getByRole('button',{name:'取消作业',exact:true})).toHaveCount(0);
});
