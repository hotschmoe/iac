/* IN AMBER CLAD prototype runtime: mock world, 1 Hz tick, shared shell, sfx. No dependencies. */
(function(){var l=document.createElement('link');l.rel='stylesheet';l.href='https://fonts.googleapis.com/css2?family=Chakra+Petch:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500;700&family=Newsreader:ital,wght@1,400;1,500&display=swap';document.head.appendChild(l)})();
window.IAC=(function(){
const SQ3=Math.sqrt(3);
/* flat-top axial hexes. Directions are named by SCREEN position, and the vector is the contract
   (the TUI/server name E=(1,0); on a flat-top map that is lower-right, hence the old compass confusion). */
const DIRS=[
 {i:0,n:'N', v:[0,-1],key:'w',lab:'Up'},
 {i:1,n:'NE',v:[1,-1],key:'e'},
 {i:2,n:'SE',v:[1,0], key:'d'},
 {i:3,n:'S', v:[0,1], key:'s'},
 {i:4,n:'SW',v:[-1,1],key:'a'},
 {i:5,n:'NW',v:[-1,0],key:'q'}];
const px=(q,r,s=1)=>[s*1.5*q, s*(SQ3/2*q+SQ3*r)];
const dist=(q,r)=>(Math.abs(q)+Math.abs(r)+Math.abs(q+r))/2;
const hd=(a,b)=>dist(a.q-b.q,a.r-b.r);
const K=(q,r)=>q+','+r;
function hash(a,b,c=0){let h=(Math.imul(a|0,374761393)+Math.imul(b|0,668265263)+Math.imul(c|0,2147483629))|0;h=Math.imul(h^(h>>>13),1274126177);h^=h>>>16;return (h>>>0)/4294967296}
const zoneOf=d=>d===0?'Central Hub':d<=8?'Inner Ring':d<=20?'Outer Ring':'The Wandering';
const POWER={scout:8,corvette:25,frigate:70,cruiser:160,hauler:6};
const STATS={scout:{hull:30,shield:10,weapon:5,cargo:20},corvette:{hull:50,shield:20,weapon:15,cargo:10},frigate:{hull:120,shield:60,weapon:30,cargo:30},cruiser:{hull:200,shield:100,weapon:80,cargo:50},hauler:{hull:80,shield:20,weapon:5,cargo:200}};
const HOME={q:4,r:-2}, S0={q:6,r:-4};

/* ---------------- world ---------------- */
const cache=new Map(), forced=new Map(), over=new Map();
function tierComp(t,h){
  if(t===1)return h<.5?[['scout',1]]:[['scout',2]];
  if(t===2)return [['corvette',2+(h>.6?1:0)]];
  if(t===3)return [['frigate',1],['corvette',2+(h>.5?1:0)]];
  if(t===4)return [['cruiser',1],['frigate',2],['corvette',3+(h>.5?2:0)]];
  return [];
}
function sector(q,r){
  const k=K(q,r); if(cache.has(k))return cache.get(k);
  const d=dist(q,r), x=hash(q,r,1);
  const terrain=x<.42?'empty':x<.64?'asteroid':x<.78?'nebula':x<.9?'debris':'anomaly';
  const rich=1.2+d*.11;
  const o=(s,bonus)=>Math.max(0,Math.min(4,Math.floor(hash(q,r,s)*rich+bonus)));
  const ore={m:o(2,terrain==='asteroid'?1:0),c:o(3,terrain==='asteroid'?.6:0),d:o(4,terrain==='nebula'?1:0)};
  let threat=0;
  if(hd({q,r},HOME)>2){ const p=Math.min(.7,.10+d*.028); if(hash(q,r,5)<p) threat=Math.max(1,Math.min(4,1+Math.floor(hash(q,r,6)*(1+d/5)))); }
  const comp=tierComp(threat,hash(q,r,7));
  const behavior=threat===0?null:threat===1?'passive':threat===2?'patrol':threat===3?'aggressive':'swarm';
  const power=comp.reduce((a,[c,n])=>a+POWER[c]*n,0);
  const salv=hash(q,r,8)<.1?{m:Math.round(20+hash(q,r,9)*60+d*4),c:Math.round(5+hash(q,r,10)*20),d:Math.round(hash(q,r,11)*12)}:null;
  const site=hash(q,r,12)<.05+d*.004&&d>2?{tier:Math.max(1,Math.min(4,1+Math.floor(d/7))),risk:['quiet','uneasy','hot'][Math.floor(hash(q,r,13)*3)]}:null;
  const s={q,r,d,zone:zoneOf(d),terrain,ore,threat,comp,behavior,power,salvage:salv,site,
    loot:1+Math.min(3,Math.floor(d/6))};   /* loot multiplier tier: distance-scaled loot (server addition) */
  Object.assign(s,over.get(k)||{});
  cache.set(k,s);return s;
}
function parent(q,r){
  const d=dist(q,r); if(!d)return null;
  const c=DIRS.map(v=>({q:q+v.v[0],r:r+v.v[1]})).filter(n=>dist(n.q,n.r)===d-1);
  return c[Math.floor(hash(q,r,20)*c.length)];
}
function hasLane(a,b){
  const ek=a.q<b.q||(a.q===b.q&&a.r<b.r)?K(a.q,a.r)+'|'+K(b.q,b.r):K(b.q,b.r)+'|'+K(a.q,a.r);
  if(forced.has(ek))return forced.get(ek);
  const pa=parent(a.q,a.r),pb=parent(b.q,b.r);
  if((pa&&pa.q===b.q&&pa.r===b.r)||(pb&&pb.q===a.q&&pb.r===a.r))return true;
  const m=Math.max(dist(a.q,a.r),dist(b.q,b.r));
  return hash(a.q+b.q*7,a.r+b.r*13,21)<(m<=1?1:m<=8?.62:.45);
}
function forceLane(a,b,v){const ek=a.q<b.q||(a.q===b.q&&a.r<b.r)?K(a.q,a.r)+'|'+K(b.q,b.r):K(b.q,b.r)+'|'+K(a.q,a.r);forced.set(ek,v)}
function neighbors(q,r){return DIRS.map(D=>{const n={q:q+D.v[0],r:r+D.v[1]};return {dir:D.i,q:n.q,r:n.r,lane:hasLane({q,r},n)}})}
function lanesOf(q,r){return neighbors(q,r).filter(n=>n.lane)}

/* hand-authored opening neighbourhood so the demo tells a story */
function set(q,r,o){over.set(K(q,r),o);cache.delete(K(q,r))}
set(6,-4,{terrain:'asteroid',ore:{m:3,c:2,d:0},threat:0,comp:[],behavior:null,power:0,salvage:{m:60,c:15,d:9},site:{tier:2,risk:'uneasy'}});
set(6,-5,{terrain:'empty',ore:{m:1,c:0,d:1},threat:2,comp:[['corvette',2]],behavior:'patrol',power:50,salvage:null,site:null});         // N
set(7,-5,{terrain:'nebula',ore:{m:0,c:1,d:3},threat:0,comp:[],behavior:null,power:0,salvage:null,site:null});                              // NE
set(7,-4,{terrain:'debris',ore:{m:2,c:0,d:0},threat:0,comp:[],behavior:null,power:0,salvage:{m:40,c:10,d:4},site:null});                    // SE (stale)
set(5,-3,{terrain:'empty',ore:{m:1,c:1,d:0},threat:0,comp:[],behavior:null,power:0,salvage:null,site:null});                                // SW toward home
set(5,-4,{terrain:'anomaly',ore:{m:0,c:3,d:1},threat:3,comp:[['frigate',1],['corvette',2]],behavior:'aggressive',power:120,salvage:null,site:null}); // NW unscanned
set(3,-1,{terrain:'asteroid',ore:{m:2,c:1,d:0},threat:0,comp:[],behavior:null,power:0,salvage:null,site:null});
set(8,-6,{terrain:'asteroid',ore:{m:4,c:2,d:1},threat:3,comp:[['frigate',1],['corvette',2]],behavior:'aggressive',power:120,salvage:null,site:{tier:3,risk:'hot'}});
set(9,-7,{terrain:'empty',ore:{m:1,c:1,d:1},threat:4,comp:[['cruiser',1],['frigate',2],['corvette',3]],behavior:'swarm',power:440,salvage:null,site:null});
for(const D of DIRS){const n={q:6+D.v[0],r:-4+D.v[1]};forceLane(S0,n,D.n!=='S')}

/* ---------------- state ---------------- */
const CAP={m:12000,c:7000,d:3500};            /* storage caps: server addition */
const PROD={m:2.46,c:1.12,d:0.43};            /* per tick */
let T0=Date.now(); let R0={m:7340,c:3105,d:880}, Rt=1842;
const save=()=>{try{localStorage.setItem('iac-proto',JSON.stringify({T0,R0,Rt,seen:[...seen],f1:fleets[0].loc,route:fleets[0].route,visited:[...visited]}))}catch(e){}};
const seen=new Map(), visited=new Set();
const fleets=[
 {id:1,name:'Vanguard',loc:{q:6,r:-4},state:'Idle',ships:[['corvette',3],['scout',1],['hauler',1]],fuel:96,fuelMax:120,fuelPerHop:5,cargo:{m:34,c:12,d:0},cap:250,cd:0,policy:'manual',route:[],params:{min_fuel_pct:25,cargo_return_pct:90,max_range:6,engage_ratio_x10:15}},
 {id:2,name:'Prospector',loc:{q:3,r:-1},state:'Harvesting',ships:[['scout',2]],fuel:44,fuelMax:60,fuelPerHop:3,cargo:{m:19,c:6,d:0},cap:40,cd:0,policy:'mine_and_return',route:[],params:{min_fuel_pct:25,cargo_return_pct:90,max_range:4,engage_ratio_x10:15}},
 {id:3,name:'Longhaul',loc:{q:4,r:-2},state:'Docked',ships:[['hauler',2]],fuel:200,fuelMax:200,fuelPerHop:8,cargo:{m:0,c:0,d:0},cap:400,cd:0,policy:'manual',route:[],params:{min_fuel_pct:25,cargo_return_pct:90,max_range:6,engage_ratio_x10:15}}];
const power=f=>f.ships.reduce((a,[c,n])=>a+POWER[c]*n,0);
const HW={
  buildings:[['Metal Mine',9],['Crystal Mine',7],['Deuterium Synthesizer',5],['Shipyard',4],['Research Lab',3],['Fuel Depot',2],['Sensor Array',3],['Defense Grid',2]],
  queueA:{name:'Crystal Mine',to:8,start:-420,end:260},   /* relative to tick at load */
  queueB:null,                                           /* second slot, locked until research */
  research:{name:'Weapons Research',to:4,start:-300,end:410},
  yard:{cls:'corvette',count:3,built:1,start:-200,end:170}};
let tick=1842, lastTickAt=performance.now();
const hooks=[];
const events=[];

/* ---- intel ---- */
function initSeen(){
  const t=tick, trail=[[5,-3],[6,-4],[7,-5],[8,-6],[9,-7],[10,-8],[3,-1],[2,1],[1,3],[0,5],[-1,7],[8,-3],[10,-4],[12,-5]];
  for(let q=-14;q<=14;q++)for(let r=-14;r<=14;r++){ if(dist(q,r)>16)continue;
    const dh=hd({q,r},HOME); let age=null;
    if(dh<=3)age=30+hash(q,r,31)*500;
    else for(const [a,b] of trail){const dd=hd({q,r},{q:a,r:b}); if(dd<=1){age=100+hash(q,r,32)*900+dist(a,b)*150;break}}
    if(age==null&&dh<=6&&hash(q,r,33)<.55)age=900+hash(q,r,34)*2500;
    if(age!=null)seen.set(K(q,r),t-Math.floor(age));
  }
  /* authored opening intel */
  seen.set(K(6,-5),t-0); seen.set(K(7,-5),t-90); seen.set(K(7,-4),t-1500); seen.set(K(5,-3),t-40); seen.delete(K(5,-4)); seen.set(K(8,-6),t-2400); seen.set(K(9,-7),t-5200);
}
function intel(q,r){
  const t=seen.get(K(q,r)); if(t==null)return {known:false,age:Infinity,state:'unknown',f:0};
  const age=tick-t, state=age<=6?'live':age<=300?'fresh':age<=1800?'aging':'stale';
  return {known:true,age,state,f:Math.max(.12,1-age/6000)};
}
function refreshLive(){ for(const f of fleets){ const rng=f.ships.some(s=>s[0]==='scout')?2:1;
  for(let dq=-rng;dq<=rng;dq++)for(let dr=-rng;dr<=rng;dr++){ if(dist(dq,dr)>rng)continue; const q=f.loc.q+dq,r=f.loc.r+dr;
    if(q===5&&r===-4&&!revealed.has('nw'))continue; seen.set(K(q,r),tick);} } }
const revealed=new Set();

/* ---- pathfinding (BFS/Dijkstra over lanes, known sectors only) ---- */
function route(from,to,mode='fast'){
  const pen=mode==='safe'?[0,2,5,14,40]:mode==='balanced'?[0,1,2.5,6,16]:[0,0,0,0,0];
  const open=[{q:from.q,r:from.r,c:0}], best=new Map([[K(from.q,from.r),0]]), prev=new Map();
  const goal=K(to.q,to.r);
  while(open.length){ open.sort((a,b)=>a.c-b.c); const cur=open.shift(); const ck=K(cur.q,cur.r);
    if(ck===goal)break; if(cur.c>best.get(ck))continue;
    for(const n of lanesOf(cur.q,cur.r)){ const nk=K(n.q,n.r); if(!seen.has(nk)&&nk!==goal)continue; if(dist(n.q,n.r)>22)continue;
      const s=sector(n.q,n.r); const c=cur.c+1+pen[intel(n.q,n.r).known?s.threat:0];
      if(c<(best.get(nk)??1e9)){best.set(nk,c);prev.set(nk,ck);open.push({q:n.q,r:n.r,c})} } }
  if(!prev.has(goal)&&goal!==K(from.q,from.r))return null;
  const path=[]; let k=goal; while(k&&k!==K(from.q,from.r)){const [q,r]=k.split(',').map(Number);path.unshift({q,r});k=prev.get(k)}
  return path;
}
function routeStats(f,path){
  let fuel=0,thr=0,maxT=0,hostile=0; const hold=[];
  path.forEach((p,i)=>{const s=sector(p.q,p.r),it=intel(p.q,p.r); fuel+=f.fuelPerHop*(s.terrain==='nebula'?1.5:1);
    if(it.known&&s.threat>0){hostile++;thr+=s.threat;maxT=Math.max(maxT,s.threat); if(s.threat>=3)hold.push(i)}});
  const last=path[path.length-1]||f.loc, back=hd(last,HOME)*f.fuelPerHop;
  return {hops:path.length,fuel:Math.round(fuel),eta:path.length*2+0,threat:thr,maxT,hostile,hold,fuelLeft:Math.round(f.fuel-fuel),back:Math.round(back),
    canReturn:f.fuel-fuel>=back,canMake:f.fuel>=fuel};
}

/* ---------------- shell ---------------- */
const ic={
 overview:'<rect x="3" y="3" width="8" height="8" rx="1.5"/><rect x="13" y="3" width="8" height="5" rx="1.5"/><rect x="13" y="10" width="8" height="11" rx="1.5"/><rect x="3" y="13" width="8" height="8" rx="1.5"/>',
 windshield:'<path d="M3 17 L7 6 H17 L21 17 Z"/><path d="M12 6 V17 M5.5 12 H18.5" opacity=".6"/><circle cx="12" cy="11.5" r="2.2"/>',
 map:'<path d="M12 3 l4 2.3 v4.6 L12 12.2 8 9.9 V5.3z"/><path d="M8 13 l4 2.3 v4.6 L8 22 4 19.7 v-4.6z" opacity=".7"/><path d="M16 13 l4 2.3 v4.6 L16 22 12 19.7 v-4.6z" opacity=".7"/>',
 homeworld:'<circle cx="12" cy="12" r="6"/><ellipse cx="12" cy="12" rx="10" ry="3.2" transform="rotate(-20 12 12)"/>',
 fleets:'<path d="M12 3 L18 12 H6z"/><path d="M12 11 L19 21 H5z" opacity=".7"/>',
 comms:'<path d="M4 5h16v11H11l-5 4v-4H4z"/><path d="M8 9h8M8 12h5" opacity=".7"/>',
 rank:'<path d="M5 20V12M12 20V5M19 20v-5"/><path d="M3 20h18"/>',
 bell:'<path d="M6 17V11a6 6 0 0 1 12 0v6l2 2H4z"/><path d="M10 21h4"/>',
 sound:'<path d="M4 9v6h4l5 4V5L8 9z"/><path d="M16 9a4 4 0 0 1 0 6"/>',
 scan:'<path d="M3 5h4M17 5h4M3 19h4M17 19h4M3 5v3M21 5v3M3 19v-3M21 19v-3"/>',
 help:'<circle cx="12" cy="12" r="9"/><path d="M9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.7.4-1 .9-1 1.7M12 17h.01"/>'};
const NAV=[['overview','Overview','overview.html','o'],['windshield','Windshield','windshield.html','w'],['map','Map','map.html','m'],['homeworld','Homeworld','homeworld.html','h'],['fleets','Fleets','#spec','f'],['comms','Comms','#spec','c'],['rank','Rank','#spec','r']];
function svg(n){return `<svg viewBox="0 0 24 24">${ic[n]}</svg>`}
const fmt=n=>Math.floor(n).toLocaleString('en-US');
const clock=t=>{t=Math.max(0,Math.floor(t));const h=Math.floor(t/3600),m=Math.floor(t%3600/60),s=t%60;return (h?h+':'+String(m).padStart(2,'0'):m)+':'+String(s).padStart(2,'0')};
const ago=t=>t<60?t+'s':t<3600?Math.floor(t/60)+'m':t<86400?Math.floor(t/3600)+'h':Math.floor(t/86400)+'d';
function res(){const dt=tick-Rt;return {m:Math.min(CAP.m,R0.m+PROD.m*dt),c:Math.min(CAP.c,R0.c+PROD.c*dt),d:Math.min(CAP.d,R0.d+PROD.d*dt)}}
function spend(cost){const r=res();if(r.m<(cost.m||0)||r.c<(cost.c||0)||r.d<(cost.d||0))return false;R0={m:r.m-(cost.m||0),c:r.c-(cost.c||0),d:r.d-(cost.d||0)};Rt=tick;save();return true}
function mount(active,opts={}){
  const body=document.body;
  body.innerHTML=`<div class="app">
   <header class="hud">
    <a class="logo" href="index.html">${hexLogo()}<div><span>IN AMBER CLAD</span><small>ADMIRAL'S CONSOLE</small></div></a>
    <div class="res" id="hudres"></div>
    <div class="hud-r">
      <div class="tick"><svg viewBox="0 0 24 24"><circle class="bg" cx="12" cy="12" r="9"/><circle id="tring" class="fg" cx="12" cy="12" r="9" stroke-dasharray="56.5" stroke-dashoffset="56.5"/></svg><span>TICK <b id="tickn" class="num" style="color:var(--a300)">${tick}</b></span></div>
      <button class="ibtn" id="bsnd" title="Sound (M)">${svg('sound')}</button>
      <button class="ibtn" id="bhlp" title="Keys (?)">${svg('help')}</button>
      <button class="ibtn" id="bbell" title="Alerts">${svg('bell')}<span class="dot" id="beldot" style="display:none"></span></button>
      <div class="who"><b>Designer</b>Inner Ring &middot; rank #14</div>
    </div>
   </header>
   <nav class="rail">${NAV.map(([id,l,h,k],i)=>`<a href="${h}" class="${id===active?'on':''} ${h==='#spec'?'spec':''}" data-id="${id}" title="${l} (G then ${k.toUpperCase()})">${svg(id)}<span>${l}</span><kbd>${k}</kbd>${opts.badges&&opts.badges[id]?`<b class="badge">${opts.badges[id]}</b>`:''}</a>`).join('')}<div class="sp"></div></nav>
   <main class="main" id="main"></main>
  </div><div class="toasts" id="toasts"></div><div id="help"></div>`;
  hudUpdate(); document.getElementById('bsnd').onclick=toggleSound;
  document.getElementById('bhlp').onclick=toggleHelp; document.getElementById('bbell').onclick=()=>toast('Alerts','Raid warning, queue idle and fleet-lost alerts raise a badge here and a browser notification.','red');
  document.querySelectorAll('.rail a.spec').forEach(a=>a.onclick=e=>{e.preventDefault();toast('Spec only','This screen is designed in spec.md (section 5) but not prototyped.','violet')});
  addEventListener('keydown',globalKeys);
  requestAnimationFrame(function ring(){const e=document.getElementById('tring');if(e){const p=Math.min(1,(performance.now()-lastTickAt)/1000);e.style.strokeDashoffset=56.5*(1-p)}requestAnimationFrame(ring)});
  return document.getElementById('main');
}
function hexLogo(){return `<svg viewBox="0 0 32 32"><defs><radialGradient id="lg" cx="40%" cy="35%"><stop offset="0" stop-color="#ffe2a3"/><stop offset=".6" stop-color="#ffb000"/><stop offset="1" stop-color="#9a6500"/></radialGradient></defs><path d="M16 2l12 7v14l-12 7L4 23V9z" fill="url(#lg)" opacity=".92"/><path d="M16 2l12 7v14l-12 7L4 23V9z" fill="none" stroke="#fff3d6" stroke-opacity=".5"/><path d="M12 19l4-9 4 9h-3l-1-3-1 3z" fill="#2a1900"/><circle cx="21" cy="12" r="1.2" fill="#2a1900"/></svg>`}
function hudUpdate(){
  const r=res(), el=document.getElementById('hudres'); if(!el)return;
  const items=[['m','Metal','--metal'],['c','Crystal','--crystal'],['d','Deuterium','--deut']];
  if(!el.children.length)el.innerHTML=items.map(([k,l,c])=>`<div class="res-chip" style="--c:var(${c})" data-k="${k}" title="${l}"><i></i><b class="num"></b><em class="num"></em><span class="cap"></span></div>`).join('');
  items.forEach(([k],i)=>{const ch=el.children[i];ch.querySelector('b').textContent=fmt(r[k]);ch.querySelector('em').textContent='+'+PROD[k].toFixed(1)+'/t';ch.querySelector('.cap').style.width=(r[k]/CAP[k]*100)+'%';ch.classList.toggle('full',r[k]>=CAP[k]*.98)});
  const t=document.getElementById('tickn'); if(t)t.textContent=tick;
}
function toast(title,msg,cls=''){const w=document.getElementById('toasts');if(!w)return;const d=document.createElement('div');d.className='toast '+cls;d.innerHTML=`<b>${title}</b>${msg}`;w.appendChild(d);while(w.children.length>4)w.firstChild.remove();setTimeout(()=>{d.style.transition='opacity .4s';d.style.opacity=0;setTimeout(()=>d.remove(),400)},4200)}
function toggleHelp(){const h=document.getElementById('help');if(h.innerHTML){h.innerHTML='';return}
 const rows=opts_help();h.innerHTML=`<div style="position:fixed;inset:0;z-index:90;background:rgba(5,3,2,.78);display:grid;place-items:center" onclick="this.parentNode.innerHTML=''"><div class="panel" style="padding:20px 26px;min-width:440px;max-width:92vw"><div class="ph" style="padding-left:0">Keys <span class="sub">press ? or Esc to close</span></div>${rows}</div></div>`}
function opts_help(){const R=(k,t)=>`<div style="display:flex;gap:10px;padding:3px 0;align-items:center"><span style="min-width:96px">${k.split(' ').map(x=>`<span class="key">${x}</span>`).join(' ')}</span><span style="color:var(--text-2)">${t}</span></div>`;
 return R('G O','Go to Overview')+R('G W','Windshield')+R('G M','Map')+R('G H','Homeworld')+R('1 2 3','Select fleet (control groups)')+R('Tab','Cycle fleet')+
 '<hr style="border:0;border-top:1px solid var(--line);margin:8px 0">'+R('Q W E','Jump through gate: NW, N, NE')+R('A S D','Jump: SW, S, SE')+R('V','Scan (radar pulse)')+R('H','Harvest')+R('F','Fire: engage hostiles')+R('C','Collect salvage')+R('B','Board derelict')+R('R','Recall home')+R('X','Stop')+'<hr style="border:0;border-top:1px solid var(--line);margin:8px 0">'+R('M','Mute / unmute')+R('L','Map: threat layer')+R('Enter','Map: engage route')+R('Shift+Click','Map: add waypoint')+R('Esc','Cancel / close')}
let gpend=0;
function globalKeys(e){
  if(e.target&&/INPUT|TEXTAREA/.test(e.target.tagName))return;
  const k=e.key.toLowerCase();
  if(k==='?'){toggleHelp();return}
  if(k==='escape'){const h=document.getElementById('help');if(h)h.innerHTML=''}
  if(k==='m'&&!window.IAC_NO_M){toggleSound();return}
  if(gpend&&performance.now()-gpend<1500){const t=NAV.find(n=>n[3]===k&&n[2]!=='#spec');gpend=0;if(t){location.href=t[2]}return}
  if(k==='g'){gpend=performance.now();toast('Go to','O verview, W indshield, M ap, H omeworld','')}
}
/* ---------------- sfx (off until first click on the speaker) ---------------- */
let ac=null,muted=true;
function toggleSound(){muted=!muted;if(!muted&&!ac){try{ac=new (window.AudioContext||window.webkitAudioContext)()}catch(e){}}toast('Sound',muted?'Muted':'On')}
function tone(f0,f1,dur,type='sine',vol=.06,delay=0){if(muted||!ac)return;const t=ac.currentTime+delay,o=ac.createOscillator(),g=ac.createGain();o.type=type;o.frequency.setValueAtTime(f0,t);o.frequency.exponentialRampToValueAtTime(Math.max(20,f1),t+dur);g.gain.setValueAtTime(vol,t);g.gain.exponentialRampToValueAtTime(.0001,t+dur);o.connect(g).connect(ac.destination);o.start(t);o.stop(t+dur+.02)}
function sfx(n){({click:()=>tone(900,700,.05,'square',.03),jump:()=>{tone(90,1400,.7,'sawtooth',.05);tone(60,300,.8,'sine',.08)},scan:()=>{tone(700,700,.9,'sine',.07);tone(1400,1400,.6,'sine',.03,.15)},
 hit:()=>tone(220,60,.12,'square',.05),fire:()=>tone(1200,200,.14,'sawtooth',.03),alert:()=>{tone(520,520,.15,'square',.05);tone(390,390,.25,'square',.05,.18)},ok:()=>{tone(660,990,.12,'triangle',.06);tone(990,1320,.14,'triangle',.05,.1)},harvest:()=>tone(300,420,.06,'triangle',.03)}[n]||(()=>{}))()}
/* ---------------- tick ---------------- */
function pushEvent(kind,text,cls=''){events.unshift({tick,kind,text,cls});if(events.length>60)events.pop()}
function start(){
  initSeen(); try{const s=JSON.parse(localStorage.getItem('iac-proto')||'null');if(s&&s.f1&&false){}}catch(e){}
  setInterval(()=>{tick++;lastTickAt=performance.now();refreshLive();hudUpdate();for(const h of hooks)h(tick)},1000);
  refreshLive();
}
const IACobj={DIRS,px,dist,hd,K,hash,zoneOf,sector,neighbors,lanesOf,hasLane,seen,intel,fleets,power,POWER,STATS,HOME,S0,CAP,PROD,res,spend,route,routeStats,HW,
  get tick(){return tick},set tick(v){tick=v},onTick:f=>hooks.push(f),mount,toast,sfx,fmt,clock,ago,pushEvent,events,start,hudUpdate,revealed,visited,svg,zoneName:zoneOf,
  get lastTickAt(){return lastTickAt}, setMuted(v){muted=v}};
return IACobj})();
