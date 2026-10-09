/* IN AMBER CLAD prototype runtime: mock world, 1 Hz tick, shared shell, sfx. No dependencies. */
(function(){var l=document.createElement('link');l.rel='stylesheet';l.href='https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500;600&family=IBM+Plex+Sans+Condensed:wght@400;500;600;700&display=swap';document.head.appendChild(l)})();
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
const POWER={scout:9,corvette:22,frigate:48,cruiser:110,hauler:15};   /* weapon + (hull+shield)/10, economy spec 5.1 */
const ratingOf=p=>Math.max(1,Math.min(9,1+Math.floor(Math.log2(Math.max(1,p)/4))));   /* threat T1..T9, economy spec 7.2 */
function ratioInfo(mine,theirs){const r=mine/Math.max(1,theirs);return r>=3?{r,l:'SAFE',c:'safe',p:96}:r>=2?{r,l:'FAVOURABLE',c:'fav',p:80}:r>=1.2?{r,l:'RISKY',c:'risky',p:52}:{r,l:'DEADLY',c:'deadly',p:r>=.9?34:14}}
const STATS={scout:{hull:30,shield:10,weapon:5,cargo:20},corvette:{hull:50,shield:20,weapon:15,cargo:10},frigate:{hull:120,shield:60,weapon:30,cargo:30},cruiser:{hull:200,shield:100,weapon:80,cargo:50},hauler:{hull:80,shield:20,weapon:5,cargo:200}};
const HOME={q:4,r:-2}, S0={q:6,r:-4};

/* ---------------- world ---------------- */
const cache=new Map(), forced=new Map(), over=new Map();
function sector(q,r){
  const k=K(q,r); if(cache.has(k))return cache.get(k);
  const d=dist(q,r), x=hash(q,r,1);
  const terrain=x<.42?'empty':x<.64?'asteroid':x<.78?'nebula':x<.9?'debris':'anomaly';
  const rich=1.2+d*.11;
  const o=(s,bonus)=>Math.max(0,Math.min(4,Math.floor(hash(q,r,s)*rich+bonus)));
  const ore={m:o(2,terrain==='asteroid'?1:0),c:o(3,terrain==='asteroid'?.6:0),d:o(4,terrain==='nebula'?1:0)};
  /* NPC group per economy spec 7.1: class by distance, power 4*1.22^(d-1), presence 25%+2.5%/ring */
  let threat=0,comp=[],behavior=null,power=0,m=1;
  if(hd({q,r},HOME)>2&&hash(q,r,5)<Math.min(.85,.25+.025*d)){
    const cls=d<=7?'scout':d<=12?'corvette':d<=20?'frigate':'cruiser'; m=Math.min(1.3,.6+.02*d);
    const n=Math.max(1,Math.min(12,Math.round(4*Math.pow(1.22,d-1)/(POWER[cls]*m)))); comp=[[cls,n]];
    power=Math.round(POWER[cls]*n*m);
    behavior=hash(q,r,6)<Math.max(0,.5-.08*(d-8))?'passive':power>=256?'swarm':power>=64?'aggressive':'patrol';
    threat=ratingOf(power*(behavior==='aggressive'||behavior==='swarm'?1.25:1));}
  const salv=hash(q,r,8)<.1?{m:Math.round(20+hash(q,r,9)*60+d*4),c:Math.round(5+hash(q,r,10)*20),d:Math.round(hash(q,r,11)*12)}:null;
  const site=hash(q,r,12)<.05+d*.004&&d>2?{tier:d<=11?1:d<=21?2:3,risk:['quiet','uneasy','hot'][Math.floor(hash(q,r,13)*3)]}:null;
  const s={q,r,d,zone:zoneOf(d),terrain,ore,threat,comp,behavior,power,m,salvage:salv,site,
    loot:1+Math.min(3,Math.floor(d/6))};
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
set(6,-4,{terrain:'asteroid',ore:{m:3,c:2,d:0},threat:0,comp:[],behavior:null,power:0,salvage:{m:60,c:15,d:9},site:{tier:1,risk:'uneasy'}});
set(6,-5,{terrain:'empty',ore:{m:1,c:0,d:1},threat:3,comp:[['corvette',1]],behavior:'patrol',m:.72,power:16,salvage:null,site:null});          // N
set(7,-5,{terrain:'nebula',ore:{m:0,c:1,d:3},threat:0,comp:[],behavior:null,power:0,salvage:null,site:null});                              // NE
set(7,-4,{terrain:'debris',ore:{m:2,c:0,d:0},threat:0,comp:[],behavior:null,power:0,salvage:{m:40,c:10,d:4},site:null});                    // SE (stale)
set(5,-3,{terrain:'empty',ore:{m:1,c:1,d:0},threat:0,comp:[],behavior:null,power:0,salvage:null,site:null});                                // SW toward home
set(5,-4,{terrain:'anomaly',ore:{m:0,c:3,d:1},threat:5,comp:[['frigate',1],['corvette',2]],behavior:'aggressive',m:1,power:92,salvage:null,site:null}); // NW unscanned
set(3,-1,{terrain:'asteroid',ore:{m:2,c:1,d:0},threat:0,comp:[],behavior:null,power:0,salvage:null,site:null});
set(8,-6,{terrain:'asteroid',ore:{m:4,c:2,d:1},threat:4,comp:[['corvette',3]],behavior:'patrol',m:.76,power:50,salvage:null,site:{tier:1,risk:'hot'}});
set(9,-7,{terrain:'empty',ore:{m:1,c:1,d:1},threat:7,comp:[['cruiser',1],['frigate',2],['corvette',3]],behavior:'swarm',m:1,power:272,salvage:null,site:null});
for(const D of DIRS){const n={q:6+D.v[0],r:-4+D.v[1]};forceLane(S0,n,D.n!=='S')}

/* ---------------- state ---------------- */
const CAP={m:11250,c:7875,d:5625};            /* Storage Vault L2 at pace 1: 5000/3500/2500 x1.5^2 (economy spec 3.1) */
const VAULT=2;
const PROD={m:2.46,c:1.12,d:0.43};            /* per tick */
let T0=Date.now(); let R0={m:7340,c:6900,d:880}, Rt=1842;
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
  const pen=t=>!t?0:mode==='safe'?2+t*t*.7:mode==='balanced'?1+t*t*.22:0;
  const open=[{q:from.q,r:from.r,c:0}], best=new Map([[K(from.q,from.r),0]]), prev=new Map();
  const goal=K(to.q,to.r);
  while(open.length){ open.sort((a,b)=>a.c-b.c); const cur=open.shift(); const ck=K(cur.q,cur.r);
    if(ck===goal)break; if(cur.c>best.get(ck))continue;
    for(const n of lanesOf(cur.q,cur.r)){ const nk=K(n.q,n.r); if(!seen.has(nk)&&nk!==goal)continue; if(dist(n.q,n.r)>22)continue;
      const s=sector(n.q,n.r); const c=cur.c+1+pen(intel(n.q,n.r).known?s.threat:0);
      if(c<(best.get(nk)??1e9)){best.set(nk,c);prev.set(nk,ck);open.push({q:n.q,r:n.r,c})} } }
  if(!prev.has(goal)&&goal!==K(from.q,from.r))return null;
  const path=[]; let k=goal; while(k&&k!==K(from.q,from.r)){const [q,r]=k.split(',').map(Number);path.unshift({q,r});k=prev.get(k)}
  return path;
}
function routeStats(f,path){
  let fuel=0,thr=0,maxT=0,hostile=0; const hold=[];
  path.forEach((p,i)=>{const s=sector(p.q,p.r),it=intel(p.q,p.r); fuel+=f.fuelPerHop*(s.terrain==='nebula'?1.5:1);
    if(it.known&&s.threat>0){hostile++;thr+=s.threat;maxT=Math.max(maxT,s.threat); if(s.threat>=5)hold.push(i)}});
  const last=path[path.length-1]||f.loc, back=hd(last,HOME)*f.fuelPerHop;
  return {hops:path.length,fuel:Math.round(fuel),eta:path.length*2+0,threat:thr,maxT,hostile,hold,fuelLeft:Math.round(f.fuel-fuel),back:Math.round(back),
    canReturn:f.fuel-fuel>=back,canMake:f.fuel>=fuel};
}

/* ---------------- symbol set: one vector path per symbol, +-50 box, nose up. Used by SVG (DOM) and Path2D (canvas). ---------------- */
const SYM={
 own:{ /* friendly hulls: symmetric, drafted, spar and bulkhead lines */
  scout:{o:'M0-46L12-6L26 34L9 24L0 36L-9 24L-26 34L-12-6Z',d:'M0-30V24M-12-6H12'},
  corvette:{o:'M0-48L10-24L14 4L34 30V42L14 34L8 44H-8L-14 34L-34 42V30L-14 4L-10-24Z',d:'M-10-24H10M-14 4H14M0-48V44M-4-34H4'},
  frigate:{o:'M0-50L9-34L12-8L40 12V36L16 30L12 46H-12L-16 30L-40 36V12L-12-8L-9-34Z',d:'M-9-34H9M-12-8H12M0-50V46M-40 12V4M40 12V4M-26 20H26'},
  cruiser:{o:'M0-52L8-40L11-14L22-8L44 14V40L20 34L14 50H-14L-20 34L-44 40V14L-22-8L-11-14L-8-40Z',d:'M-11-14H11M-14 10H14M-14 30H14M0-52V50M-34 22H-26V28H-34ZM26 22H34V28H26Z'},
  hauler:{o:'M-16-44H16L22-30V40L14 46H-14L-22 40V-30Z',d:'M-12-30H12V-8H-12ZM-12 0H12V30H-12ZM0-44V-30M0 30V46'}},
 hostile:{ /* hostile hulls: swept, barbed, asymmetric-looking; always drawn with a diamond frame on maps */
  scout:{o:'M0-44L16-4L30-14L22 30L0 14L-22 30L-30-14L-16-4Z',d:'M0-44V14M-16-4H16'},
  corvette:{o:'M0-46L14-18L38-34L30 8L12 26L0 16L-12 26L-30 8L-38-34L-14-18Z',d:'M0-46V16M-14-18H14M-30 8L-12 0M30 8L12 0'},
  frigate:{o:'M0-48L10-26L36-44L44-6L28 22L12 40L0 28L-12 40L-28 22L-44-6L-36-44L-10-26Z',d:'M0-48V28M-10-26H10M-28 22L-16 6M28 22L16 6M-44-6L-28-10M44-6L28-10'},
  cruiser:{o:'M0-52L12-30L30-50L50-14L46 20L24 44L8 36L0 50L-8 36L-24 44L-46 20L-50-14L-30-50L-12-30Z',d:'M0-52V50M-12-30H12M-30-50L-24-14M30-50L24-14M-46 20L-22 8M46 20L22 8M-8 36H8'},
  hauler:{o:'M-16-44H16L22-30V40L14 46H-14L-22 40V-30Z',d:'M-12-30H12V30H-12ZM0-44V46'}},
 /* contact / object symbols */
 diamond:'M0-34L34 0L0 34L-34 0Z',                       /* hostile frame */
 fleet:'M0-30L24 24L0 12L-24 24Z',                       /* own fleet marker (map) */
 salvage:'M-10 0A10 10 0 1 0 10 0A10 10 0 1 0-10 0M-26 0H-14M14 0H26M0-26V-14M0 14V26M-3 0H3M0-3V3',
 ore:'M-24-6L-8-26L14-22L26 0L14 24L-14 22Z',
 oreD:'M-8-26L-2 0L26 0M-2 0L14 24M-24-6L-2 0',
 derelict:'M-46 4L-26-14L-8-14M6-14L22-14L48-6L38 12L24 16M10 18L-28 16L-46 4',
 derelictD:'M-2-22L5-8L-4 4L3 24M-24-2H-14M10 0H20M26 2H34',
 home:'M-18 0A18 18 0 1 0 18 0A18 18 0 1 0-18 0M-42 6A42 15 -20 1 0 42-6A42 15 -20 1 0-42 6M0-18V18M-18 0H18',
 hub:'M-10 0A10 10 0 1 0 10 0A10 10 0 1 0-10 0M-24 0A24 24 0 1 0 24 0A24 24 0 1 0-24 0M0-24V-44M0 24V44M-24 0H-44M24 0H44M-17-17L-31-31M17-17L31-31M-17 17L-31 31M17 17L31 31',
 gate:'M-34-14V14M34-14V14M-14-14L0 0L-14 14M4-14L18 0L4 14',
 unknown:'M-26-26V-34H-18M18-34H26V-26M26 26V34H18M-18 34H-26V26M-8-10C-8-22 10-22 10-10C10-2 0-2 0 8M0 18V22'
};
function sym(name,o={}){ /* SVG string; name like 'own.corvette' or 'diamond' */
  const [a,b]=name.split('.'),e=b?SYM[a][b]:SYM[a],c=o.c||'currentColor',w=o.w||1.4,s=o.s||18;
  const paths=typeof e==='string'?`<path d="${e}"/>`:`<path d="${e.o}"/>${e.d?`<path d="${e.d}" opacity=".55"/>`:''}`;
  const extra=name==='derelict'?`<path d="${SYM.derelictD}" opacity=".7"/>`:name==='ore'?`<path d="${SYM.oreD}" opacity=".6"/>`:'';
  return `<svg class="sy ${o.cls||''}" viewBox="-50 -50 100 100" width="${s}" height="${s}" fill="none" stroke="${c}" stroke-width="${w}" stroke-linejoin="miter" stroke-linecap="butt" style="vector-effect:non-scaling-stroke;overflow:visible">${paths}${extra}</svg>`.replace(/<path /g,'<path vector-effect="non-scaling-stroke" ');
}
/* canvas: stroke a symbol path with phosphor bloom (wide additive underlay + crisp core) */
const P2={};const p2=d=>P2[d]||(P2[d]=new Path2D(d));
function vstroke(ctx,d,x,y,s,rot,col,a,lw,bloom){
  const P=p2(d),k=s/50;ctx.save();ctx.translate(x,y);ctx.rotate(rot||0);ctx.scale(k,k);ctx.lineJoin='miter';ctx.lineCap='butt';
  if(bloom!==0){ctx.globalCompositeOperation='lighter';ctx.lineWidth=lw*3.6/k;ctx.strokeStyle=`rgba(${col},${a*.12})`;ctx.stroke(P);ctx.globalCompositeOperation='source-over'}
  ctx.lineWidth=lw/k;ctx.strokeStyle=`rgba(${col},${a})`;ctx.stroke(P);ctx.restore()}
function vship(ctx,cls,hostile,x,y,s,rot,col,a,lw){const e=SYM[hostile?'hostile':'own'][cls];vstroke(ctx,e.o,x,y,s,rot,col,a,lw||1.3);vstroke(ctx,e.d,x,y,s,rot,col,a*.5,1,0)}
/* helpers for readouts */
const digits=(n,w=6)=>{let s=String(Math.max(0,Math.floor(n)));w=Math.max(w,s.length);const z=w-s.length;return (z?`<i class="z">${'0'.repeat(z)}</i>`:'')+s};
function thr(t,o={}){let h=`<span class="thr t${t} ${o.sm?'sm':''}"><b>${t?'T'+t:'--'}</b><span class="seg">`;for(let i=1;i<=9;i++)h+=`<i class="${i<=t?'f':''}"></i>`;return h+'</span></span>'}
const THR_NAME=['CLEAR','T1','T2','T3','T4','T5','T6','T7','T8','T9'];
function ratioGauge(r){const p=Math.min(4,r)/4*100;return `<div class="rg"><span class="z1"></span><span class="z2"></span><span class="z3"></span><span class="z4"></span><i style="left:${p}%"></i><em><b style="left:30%">1.2</b><b style="left:50%">2.0</b><b style="left:75%">3.0</b></em></div>`}
/* dial: 240-degree instrument dial with ticks, red sector, needle. frac 0..1 */
function dial(o){const cx=60,cy=56,R=44,A0=150,SW=240,pt=(a,r)=>[cx+r*Math.cos(a*Math.PI/180),cy+r*Math.sin(a*Math.PI/180)];
  const arc=(f0,f1,r)=>{const a=pt(A0+SW*f0,r),b=pt(A0+SW*f1,r);return `M${a[0].toFixed(1)} ${a[1].toFixed(1)}A${r} ${r} 0 ${(f1-f0)*SW>180?1:0} 1 ${b[0].toFixed(1)} ${b[1].toFixed(1)}`};
  let t='';for(let i=0;i<=24;i++){const a=A0+SW*i/24,mj=i%4===0,p1=pt(a,R),p2_=pt(a,R-(mj?8:4));t+=`M${p1[0].toFixed(1)} ${p1[1].toFixed(1)}L${p2_[0].toFixed(1)} ${p2_[1].toFixed(1)}`}
  const warn=o.warn||0.2,rot=A0+SW*Math.max(0,Math.min(1,o.frac));
  return `<svg class="dial" id="${o.id||''}" viewBox="0 0 120 92" width="${o.w||120}"><path d="${arc(0,1,R+4)}" stroke="var(--line)" fill="none" stroke-width="1"/><path d="${t}" stroke="var(--a600)" stroke-width="1" fill="none"/>
   <path d="${arc(0,warn,R-11)}" stroke="var(--ember)" stroke-width="3" fill="none"/><path d="${arc(warn,1,R-11)}" stroke="${o.col||'var(--a500)'}" stroke-width="1" fill="none" opacity=".6"/>
   <g transform="translate(${cx} ${cy})"><g class="nd" style="transform:rotate(${rot}deg);transition:transform .7s var(--ease)"><path d="M-6 0H${R-14}" stroke="var(--a100)" stroke-width="1.6" fill="none"/><path d="M${R-14} -2L${R-6} 0L${R-14} 2" fill="var(--a100)"/></g><circle r="4" fill="var(--void)" stroke="var(--a300)" stroke-width="1.2"/></g>
   <text x="${cx}" y="86" text-anchor="middle" class="dl">${o.label}</text><text x="${cx}" y="78" text-anchor="middle" class="dv" id="${o.id?o.id+'-v':''}">${o.val||''}</text></svg>`}
function dialSet(id,frac,val){const e=document.getElementById(id);if(!e)return;e.querySelector('.nd').style.transform=`rotate(${150+240*Math.max(0,Math.min(1,frac))}deg)`;const v=document.getElementById(id+'-v');if(v)v.textContent=val}

/* ---------------- shell ---------------- */
const ic={
 overview:'<path d="M3 3h8v8H3zM13 3h8v5h-8zM13 10h8v11h-8zM3 13h8v8H3z"/>',
 windshield:'<path d="M3 18L7 6H17L21 18Z"/><path d="M12 6V18M5 12H19"/><path d="M10 12H14" stroke-width="2.2"/>',
 map:'<path d="M12 3l4.5 2.5v5L12 13l-4.5-2.5v-5zM7.5 13.5L12 16v5l-4.5 2.5L3 21v-5zM16.5 13.5L21 16v5l-4.5 2.5L12 21v-5z" transform="scale(.92) translate(1 -1)"/>',
 homeworld:'<circle cx="12" cy="12" r="6"/><ellipse cx="12" cy="12" rx="10" ry="3.4" transform="rotate(-20 12 12)"/><path d="M12 6V18M6 12H18"/>',
 fleets:'<path d="M12 3L18 12H6zM12 11L19 21H5z"/>',
 comms:'<path d="M4 5H20V16H11L6 20V16H4z"/><path d="M8 9H16M8 12H13"/>',
 rank:'<path d="M5 20V12M12 20V5M19 20V15M3 20H21"/>',
 bell:'<path d="M6 17V11a6 6 0 0 1 12 0v6l2 2H4zM10 21H14"/>',
 sound:'<path d="M4 9v6h4l5 4V5L8 9zM16 9a4 4 0 0 1 0 6"/>',
 help:'<circle cx="12" cy="12" r="9"/><path d="M9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.7.4-1 .9-1 1.7M12 17h.01"/>'};
const NAV=[['overview','Overview','overview.html','o'],['windshield','Windshield','windshield.html','w'],['map','Map','map.html','m'],['homeworld','Homeworld','homeworld.html','h'],['fleets','Fleets','#spec','f'],['comms','Comms','#spec','c'],['rank','Rank','#spec','r']];
function svg(n){return `<svg viewBox="0 0 24 24">${ic[n]}</svg>`}
const fmt=n=>Math.floor(n).toLocaleString('en-US');
const clock=t=>{t=Math.max(0,Math.floor(t));const h=Math.floor(t/3600),m=Math.floor(t%3600/60),s=t%60;return (h?h+':'+String(m).padStart(2,'0'):m)+':'+String(s).padStart(2,'0')};
const ago=t=>t<60?t+'s':t<3600?Math.floor(t/60)+'m':t<86400?Math.floor(t/3600)+'h':Math.floor(t/86400)+'d';
function res(){const dt=tick-Rt;return {m:Math.min(CAP.m,R0.m+PROD.m*dt),c:Math.min(CAP.c,R0.c+PROD.c*dt),d:Math.min(CAP.d,R0.d+PROD.d*dt)}}
function spend(cost){const r=res();if(r.m<(cost.m||0)||r.c<(cost.c||0)||r.d<(cost.d||0))return false;R0={m:r.m-(cost.m||0),c:r.c-(cost.c||0),d:r.d-(cost.d||0)};Rt=tick;save();return true}
const WORLD={pace:1,preset:'PERSISTENT'};
function mount(active,opts={}){
  const body=document.body;
  body.innerHTML=`<div class="app">
   <header class="hud">
    <a class="logo" href="index.html">${hexLogo()}<div><span>IN AMBER CLAD</span><small>OPERATOR CONSOLE / MOD 4</small></div></a>
    <div class="res" id="hudres"></div>
    <div class="hud-r">
      <div class="fld" title="World pacing preset (fixed at world creation)"><span class="l">World</span><span class="v">${WORLD.preset} x${WORLD.pace}</span></div>
      <div class="tick"><span class="fld"><span class="l">Tick</span><span class="v num" id="tickn">${digits(tick)}</span></span><span class="segs" id="tsegs">${'<i></i>'.repeat(10)}</span></div>
      <div><button class="ibtn" id="bsnd" title="Sound (M)">${svg('sound')}</button><button class="ibtn" id="bhlp" title="Keys (?)">${svg('help')}</button><button class="ibtn" id="bbell" title="Alerts">${svg('bell')}<span class="dot" id="beldot" style="display:none"></span></button></div>
      <div class="who"><b>Designer</b>Inner Ring &middot; rank 014</div>
    </div>
   </header>
   <nav class="rail">${NAV.map(([id,l,h,k],i)=>`<a href="${h}" class="${id===active?'on':''} ${h==='#spec'?'spec':''}" data-id="${id}" title="${l} (G then ${k.toUpperCase()})">${svg(id)}<span>${l}</span><kbd>${k}</kbd>${opts.badges&&opts.badges[id]?`<b class="badge">${opts.badges[id]}</b>`:''}</a>`).join('')}<div class="sp"></div></nav>
   <main class="main" id="main"></main>
  </div><div class="toasts" id="toasts"></div><div id="help"></div>`;
  hudUpdate(); document.getElementById('bsnd').onclick=toggleSound;
  document.getElementById('bhlp').onclick=toggleHelp; document.getElementById('bbell').onclick=()=>toast('Alerts','Raid warning, queue idle, storage near cap and fleet-lost alerts raise a badge here and a browser notification.','red');
  document.querySelectorAll('.rail a.spec').forEach(a=>a.onclick=e=>{e.preventDefault();toast('Spec only','This screen is designed in spec.md (section 5) but not prototyped.','violet')});
  addEventListener('keydown',globalKeys);
  return document.getElementById('main');
}
function hexLogo(){return `<svg viewBox="0 0 32 32" fill="none" stroke="#ffb000" stroke-width="1.4" stroke-linejoin="miter"><path d="M16 2.5L28 9.5V22.5L16 29.5L4 22.5V9.5Z"/><path d="M9 22L16 8L23 22M12 17H20" stroke="#ffe2a3"/><path d="M16 2.5V6M16 26V29.5M4 9.5l3 1.7M28 9.5l-3 1.7" stroke-width="1" opacity=".6"/></svg>`}
function hudUpdate(){
  const r=res(), el=document.getElementById('hudres'); if(!el)return;
  const items=[['m','Fe','--metal','Metal'],['c','Cr','--crystal','Crystal'],['d','De','--deut','Deuterium']];
  if(!el.children.length)el.innerHTML=items.map(([k,l,c,n])=>`<div class="res-chip" style="--c:var(${c})" data-k="${k}" title="${n}: stock / cap (Storage Vault L${VAULT})"><div class="r1"><label>${l}</label><b class="num"></b><em class="num"></em></div><div class="bar gauge" style="--gc:var(${c})"><i></i></div></div>`).join('');
  items.forEach(([k],i)=>{const ch=el.children[i];ch.querySelector('b').innerHTML=digits(r[k],6);ch.querySelector('em').textContent='+'+PROD[k].toFixed(1)+'/T';ch.querySelector('.bar i').style.width=(r[k]/CAP[k]*100)+'%';ch.classList.toggle('full',r[k]>=CAP[k]*.98)});
  const t=document.getElementById('tickn'); if(t)t.innerHTML=digits(tick);
  const sg=document.getElementById('tsegs');if(sg){const n=Math.min(10,Math.floor((performance.now()-lastTickAt)/100)+1);[...sg.children].forEach((c,i)=>c.className=i<n?'f':'')}
}
function toast(title,msg,cls=''){const w=document.getElementById('toasts');if(!w)return;const d=document.createElement('div');d.className='toast '+cls;d.innerHTML=`<b>${title}</b>${msg}`;w.appendChild(d);while(w.children.length>4)w.firstChild.remove();setTimeout(()=>{d.style.transition='opacity .4s';d.style.opacity=0;setTimeout(()=>d.remove(),400)},4200)}
function toggleHelp(){const h=document.getElementById('help');if(h.innerHTML){h.innerHTML='';return}
 const rows=opts_help();h.innerHTML=`<div style="position:fixed;inset:0;z-index:90;background:rgba(5,3,2,.82);display:grid;place-items:center" onclick="this.parentNode.innerHTML=''"><div class="panel" style="padding:0 0 16px;min-width:440px;max-width:92vw"><div class="ph">Key reference <span class="sub">? or Esc to close</span></div><div style="padding:10px 20px 0">${rows}</div></div></div>`}
function opts_help(){const R=(k,t)=>`<div style="display:flex;gap:10px;padding:3px 0;align-items:center"><span style="min-width:96px">${k.split(' ').map(x=>`<span class="key">${x}</span>`).join(' ')}</span><span style="color:var(--text-2)">${t}</span></div>`;
 return R('G O','Go to Overview')+R('G W','Windshield')+R('G M','Map')+R('G H','Homeworld')+R('1 2 3','Select fleet (control groups)')+R('Tab','Cycle fleet')+
 '<hr style="border:0;border-top:1px solid var(--line);margin:8px 0">'+R('Q W E','Jump through gate: NW, N, NE')+R('A S D','Jump: SW, S, SE')+R('V','Scan (radar sweep)')+R('H','Harvest')+R('F','Fire: engage hostiles')+R('C','Collect salvage')+R('B','Board derelict')+R('R','Recall home')+R('X','Stop')+'<hr style="border:0;border-top:1px solid var(--line);margin:8px 0">'+R('M','Mute / unmute')+R('L','Map: threat layer')+R('Enter','Map: engage route')+R('Shift+Click','Map: add waypoint')+R('Esc','Cancel / close')}
let gpend=0;
function globalKeys(e){
  if(e.target&&/INPUT|TEXTAREA/.test(e.target.tagName))return;
  const k=e.key.toLowerCase();
  if(k==='?'){toggleHelp();return}
  if(k==='escape'){const h=document.getElementById('help');if(h)h.innerHTML=''}
  if(k==='m'&&!window.IAC_NO_M){toggleSound();return}
  if(gpend&&performance.now()-gpend<1500){const t=NAV.find(n=>n[3]===k&&n[2]!=='#spec');gpend=0;if(t){location.href=t[2]}return}
  if(k==='g'){gpend=performance.now();toast('Go to','O overview, W windshield, M map, H homeworld','')}
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
  setInterval(()=>{const sg=document.getElementById('tsegs');if(sg){const n=Math.min(10,Math.floor((performance.now()-lastTickAt)/100)+1);[...sg.children].forEach((c,i)=>c.className=i<n?'f':'')}},100);
  refreshLive();
}
const IACobj={SYM,sym,vstroke,vship,digits,thr,THR_NAME,ratioGauge,ratioInfo,ratingOf,dial,dialSet,WORLD,VAULT,DIRS,px,dist,hd,K,hash,zoneOf,sector,neighbors,lanesOf,hasLane,seen,intel,fleets,power,POWER,STATS,HOME,S0,CAP,PROD,res,spend,route,routeStats,HW,
  get tick(){return tick},set tick(v){tick=v},onTick:f=>hooks.push(f),mount,toast,sfx,fmt,clock,ago,pushEvent,events,start,hudUpdate,revealed,visited,svg,zoneName:zoneOf,
  get lastTickAt(){return lastTickAt}, setMuted(v){muted=v}};
return IACobj})();
