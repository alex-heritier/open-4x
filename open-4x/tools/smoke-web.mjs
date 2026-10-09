// Linux Chromium fallback for machines without an attached collaborative preview.
// Real WebGL rendering, keyboard orders, browser Lua, and local-storage save assertions.
import {createServer} from 'node:http';
import {readFile,writeFile,mkdtemp,stat} from 'node:fs/promises';
import {spawn} from 'node:child_process';
import {tmpdir} from 'node:os';
import {resolve,dirname,join,extname} from 'node:path';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';

const root=resolve(dirname(fileURLToPath(import.meta.url)),'../web/dist');
const evidence=await mkdtemp(join(tmpdir(),'open4x-web-'));
const mime={'.html':'text/html','.js':'text/javascript','.wasm':'application/wasm','.png':'image/png','.wav':'audio/wav','.json':'application/json'};
const server=createServer(async(req,res)=>{
  try {
    const pathname=decodeURIComponent(new URL(req.url,'http://localhost').pathname);
    const file=resolve(root,'.'+(pathname==='/'?'/index.html':pathname));
    if(!file.startsWith(root+'/'))throw new Error('path outside bundle');
    const bytes=await readFile(file);res.writeHead(200,{'Content-Type':mime[extname(file)]||'application/octet-stream'});res.end(bytes);
  }catch{res.writeHead(404);res.end();}
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
// The assertions below are written for the compact two-empire starter, not the world scenario.
const url=`http://127.0.0.1:${server.address().port}/?scenario=dawn-straits`;
const chrome=spawn(process.env.FOURX_CHROMIUM||'/usr/bin/chromium-browser',[
  '--headless','--no-sandbox','--disable-dev-shm-usage','--use-angle=swiftshader','--enable-unsafe-swiftshader',
  '--remote-debugging-port=0',`--user-data-dir=${evidence}/profile`,'about:blank',
],{stdio:['ignore','ignore','pipe']});
let chromeLog='';chrome.stderr.on('data',chunk=>chromeLog+=chunk);
let socket;
const pause=ms=>new Promise(r=>setTimeout(r,ms));
try {
  let port;
  for(let i=0;i<100;i++){try{port=Number((await readFile(`${evidence}/profile/DevToolsActivePort`,'utf8')).split('\n')[0]);break;}catch{await pause(100);}}
  if(!port)throw new Error('Chromium did not start: '+chromeLog);
  const targets=await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  socket=new WebSocket(targets.find(t=>t.type==='page').webSocketDebuggerUrl);
  await new Promise((resolve,reject)=>{socket.onopen=resolve;socket.onerror=reject;});
  let sequence=0;const pending=new Map();const errors=[];const messages=[];
  socket.onmessage=e=>{
    const m=JSON.parse(e.data);
    if(m.id){const p=pending.get(m.id);pending.delete(m.id);if(m.error)p.reject(new Error(JSON.stringify(m.error)));else p.resolve(m.result);}
    if(m.method==='Runtime.exceptionThrown')errors.push(m.params.exceptionDetails);
    if(m.method==='Runtime.consoleAPICalled')messages.push(m.params);
  };
  const call=(method,params={})=>new Promise((resolve,reject)=>{const id=++sequence;pending.set(id,{resolve,reject});socket.send(JSON.stringify({id,method,params}));});
  const evaluate=async expression=>(await call('Runtime.evaluate',{expression,returnByValue:true})).result.value;
  await call('Runtime.enable');await call('Page.enable');
  await call('Emulation.setDeviceMetricsOverride',{width:1440,height:900,deviceScaleFactor:1,mobile:false});
  await call('Page.navigate',{url});
  let ready=false;
  for(let i=0;i<180;i++){
    await pause(500);
    if(errors.length)throw new Error('Browser exception: '+JSON.stringify(errors));
    ready=await evaluate('document.querySelector("canvas")?.width === 1440');
    if(ready)break;
  }
  if(!ready)throw new Error('Game canvas did not initialize');
  await pause(5000);await evaluate('document.querySelector("canvas").focus()');
  const key=async(key,code,vk)=>{
    await call('Input.dispatchKeyEvent',{type:'keyDown',key,code,windowsVirtualKeyCode:vk});await pause(100);
    await call('Input.dispatchKeyEvent',{type:'keyUp',key,code,windowsVirtualKeyCode:vk});await pause(750);
  };
  for(const args of [['c','KeyC',67],['e','KeyE',69],['3','Digit3',51],[' ','Space',32],[' ','Space',32],['F5','F5',116]])await key(...args);
  const save=await evaluate('localStorage.getItem("open-4x-save")');
  assert.ok(save,'Browser save was not written');
  const game=JSON.parse(save).game;
  assert.equal(game.turn,3);assert.equal(Object.values(game.cities).find(c=>c.owner===1).industry,6);
  assert.equal(Object.values(game.units).filter(u=>u.owner===1).length,7);
  assert.equal(errors.length,0);
  const shot=await call('Page.captureScreenshot',{format:'png'});
  await writeFile(`${evidence}/campaign.png`,Buffer.from(shot.data,'base64'));
  await writeFile(`${evidence}/browser.log`,JSON.stringify({errors,messages},null,2));
  await writeFile(`${evidence}/dawn.save.json`,save);
  for(const [name,width,height] of [['mobile-landscape',844,390],['mobile-portrait',390,844]]) {
    await call('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});
    await pause(1500);
    if(name==='mobile-landscape') {
      await call('Emulation.setTouchEmulationEnabled',{enabled:true,maxTouchPoints:2});
      await call('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{x:777,y:35}]});await pause(100);
      await call('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});await pause(1000);
      await key('F5','F5',116);
      assert.equal(JSON.parse(await evaluate('localStorage.getItem("open-4x-save")')).game.turn,4,'Touch End Day did not work');
    }
    const shot=await call('Page.captureScreenshot',{format:'png'});
    await writeFile(`${evidence}/${name}.png`,Buffer.from(shot.data,'base64'));
  }
  assert.equal(errors.length,0);
  console.log(`PASS: browser WebGL, embedded Lua, development, recruitment, turns, and saves. Evidence: ${evidence}`);
}finally{
  socket?.close();chrome.kill();server.close();await writeFile(`${evidence}/chromium.log`,chromeLog);
}
