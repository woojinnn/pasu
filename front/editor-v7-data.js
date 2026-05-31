// editor-v7-data.js — GENERATED data + engine for Editor v7 (Scratch-style, allow-based permit).
// Embeds: glossary (en/ko display + role group), golden fixtures (samplePolicy + 3 TX),
// role-group visual map, nested-document baseline builder, evaluator, Cedar serializer.
// Reuses V6_BLOCKS / V6_TREE (editor-v6-schema.js) for the 367-block palette.


var V7_GLOSS = {"context.recipient":{"en":"Recipient","ko":"수신자","group":"address","fk":"primitive","type":"String","derived":false,"note":"출력/수령 주소"},"context.spender":{"en":"Spender","ko":"지출 승인 대상(spender)","group":"address","fk":"primitive","type":"String","derived":false,"note":"approve 받는 컨트랙트"},"context.delegatee":{"en":"Delegatee","ko":"위임 대상","group":"address","fk":"primitive","type":"String","derived":false,"note":""},"context.onBehalfOf":{"en":"On behalf of","ko":"대리 대상(onBehalfOf)","group":"address","fk":"primitive","type":"String","derived":false,"note":"제3자 대리 실행"},"context.contract":{"en":"Contract","ko":"컨트랙트 주소","group":"address","fk":"primitive","type":"String","derived":false,"note":""},"meta.from":{"en":"From (sender)","ko":"보낸 지갑","group":"address","fk":"primitive","type":"String","derived":false,"note":"principal. 동적참조 @meta.from"},"context.venue":{"en":"Venue","ko":"베뉴(거래소/풀)","group":"ref","fk":"ref","type":"AmmVenue","derived":false,"note":"27개 액션 공통"},"context.token":{"en":"Token","ko":"토큰","group":"ref","fk":"ref","type":"Core::TokenRef","derived":false,"note":"Core::TokenRef"},"context.tokenIn":{"en":"Token in","ko":"입력 토큰","group":"ref","fk":"ref","type":"Core::TokenRef","derived":false,"note":""},"context.tokenOut":{"en":"Token out","ko":"출력 토큰","group":"ref","fk":"ref","type":"Core::TokenRef","derived":false,"note":""},"context.asset":{"en":"Asset","ko":"자산","group":"ref","fk":"ref","type":"Core::TokenRef","derived":false,"note":"대출 등 대상 토큰"},"context.market":{"en":"Market","ko":"마켓","group":"ref","fk":"ref","type":"MarketRef","derived":false,"note":"Perp 마켓"},"context.platform":{"en":"Platform","ko":"플랫폼","group":"ref","fk":"ref","type":"Core::ProtocolRef","derived":false,"note":"Core::ProtocolRef"},"context.lpToken":{"en":"LP token","ko":"LP 토큰","group":"ref","fk":"ref","type":"Core::TokenRef","derived":false,"note":""},"context.nftKey":{"en":"NFT key","ko":"NFT 키","group":"ref","fk":"ref","type":"Core::TokenKey","derived":false,"note":"Core::TokenKey"},"context.amount":{"en":"Amount","ko":"수량","group":"numeric","fk":"primitive","type":"String","derived":false,"note":"14개 액션 공통"},"context.amountUsd":{"en":"Amount","ko":"수량","group":"numeric","fk":"primitive","type":"decimal","derived":true,"note":"파생 USD","unit":{"en":"USD","ko":"USD"}},"context.slippageBp":{"en":"Slippage","ko":"슬리피지","group":"numeric","fk":"primitive","type":"Long","derived":false,"note":"허용 슬리피지","unit":{"en":"bp","ko":"bp"}},"context.priceImpactBp":{"en":"Price impact","ko":"프라이스 임팩트(bp)","group":"numeric","fk":"primitive","type":"Long","derived":true,"note":"","unit":{"en":"bp","ko":"bp"}},"context.minAmountOut":{"en":"Min amount out","ko":"최소 출력 수량","group":"numeric","fk":"primitive","type":"String","derived":false,"note":"exact_input 하한"},"context.maxAmountIn":{"en":"Max amount in","ko":"최대 입력 수량","group":"numeric","fk":"primitive","type":"String","derived":false,"note":"exact_output 상한"},"context.minLpOut":{"en":"Min LP out","ko":"최소 LP 수령","group":"numeric","fk":"primitive","type":"String","derived":false,"note":""},"context.amountDesired":{"en":"Amount desired","ko":"희망 수량","group":"numeric","fk":"record","type":"{ a: String, b: String }","derived":false,"note":""},"context.maxLeverage":{"en":"Max leverage","ko":"최대 레버리지","group":"numeric","fk":"primitive","type":"String","derived":false,"note":"Perp"},"context.markPrice":{"en":"Mark price","ko":"마크 가격(markPrice)","group":"numeric","fk":"primitive","type":"String","derived":false,"note":""},"context.size":{"en":"Size","ko":"포지션 크기(size)","group":"numeric","fk":"ref","type":"SizeSpec","derived":false,"note":"SizeSpec"},"context.sellAmount":{"en":"Sell amount","ko":"매도 수량","group":"numeric","fk":"primitive","type":"String","derived":false,"note":"Intent"},"context.buyMin":{"en":"Buy min","ko":"최소 매수량","group":"numeric","fk":"primitive","type":"String","derived":false,"note":"Intent"},"context.direction.kind":{"en":"Swap direction","ko":"스왑 방향","group":"enum","fk":"primitive","type":"String","derived":false,"note":"exact_input · exact_output"},"context.rateMode":{"en":"Rate mode","ko":"금리 모드","group":"enum","fk":"primitive","type":"String","derived":false,"note":"고정/변동"},"context.side":{"en":"Side","ko":"방향(롱/숏)","group":"enum","fk":"primitive","type":"String","derived":false,"note":"Perp"},"context.orderKind":{"en":"Order kind","ko":"주문 종류","group":"enum","fk":"primitive","type":"String","derived":false,"note":"Intent"},"context.reduceOnly":{"en":"Reduce only","ko":"감소 전용(reduceOnly)","group":"enum","fk":"primitive","type":"Bool","derived":false,"note":"Bool"},"context.proof":{"en":"Merkle proof","ko":"머클 증명(proof)","group":"auth","fk":"collection","type":"Set<String>","derived":false,"note":"Set<String>"},"context.positionId":{"en":"Position ID","ko":"포지션 ID","group":"auth","fk":"primitive","type":"String","derived":false,"note":"Perp"},"enrichment.validityDeltaSec":{"en":"Time to deadline","ko":"마감까지 남은 시간","group":"derived","fk":"primitive","type":"Long","derived":true,"note":"Host-derived","unit":{"en":"sec","ko":"초"}},"enrichment.recipientIsContract":{"en":"Recipient is contract","ko":"수신자가 컨트랙트","group":"derived","fk":"primitive","type":"Bool","derived":true,"note":"Bool, Host-derived"},"enrichment.totalInputUsd":{"en":"Input value","ko":"입력 가치","group":"derived","fk":"primitive","type":"decimal","derived":true,"note":"Host-populated","unit":{"en":"USD","ko":"USD"}},"enrichment.effectiveRateVsOracleBps":{"en":"Slippage vs oracle","ko":"오라클 대비 슬리피지","group":"derived","fk":"primitive","type":"Long","derived":true,"note":"oracle","unit":{"en":"bp","ko":"bp"}},"context.expectedAmountOut":{"en":"Expected out","ko":"예상 출력","group":"derived","fk":"primitive","type":"String","derived":true,"note":"LiveField"}};
var V7_SAMPLE_POLICY = {"name":"Swap baseline","action":"Amm::Swap","effect":"permit","comment":"단일 permit. when=AND(안전조건). 위험 패턴(swap-and-send, 만료임박+고임팩트)은 NOT 배제. 매칭 실패 시 deny-by-default.","denyMessage":"swap baseline not satisfied","root":{"node":"AND","children":[{"guardId":"s1","label":"swap-and-send 배제","node":"NOT","children":[{"node":"AND","children":[{"param":"context.recipient","fieldKind":"primitive.String","op":"neq","value":"@meta.from"},{"param":"enrichment.recipientIsContract","fieldKind":"primitive.Bool","op":"isTrue"}]}]},{"guardId":"s2","label":"슬리피지 가드","param":"context.slippageBp","fieldKind":"primitive.Long","op":"lt","value":100},{"guardId":"s3","label":"만료임박+고임팩트 배제","node":"NOT","children":[{"node":"AND","children":[{"param":"enrichment.validityDeltaSec","fieldKind":"primitive.Long","op":"lt","value":30,"absence":"treatAsFalse"},{"param":"context.priceImpactBp","fieldKind":"primitive.Long","op":"gt","value":50}]}]}]},"unconnectedExamples":[{"param":"enrichment.effectiveRateVsOracleBps","fieldKind":"primitive.Long","op":"lt","value":100,"note":"오라클 슬리피지 안전조건 후보 — 미연결, 제외"},{"param":"enrichment.totalInputUsd","fieldKind":"primitive.decimal","op":"lt","value":10000,"note":"대형거래 한도 후보 — 미연결, 제외"}]};
var V7_SAMPLE_TX = [{"id":"tx-calm","label":"Calm swap · USDC→WETH","meta":{"from":"0xA1c4000000000000000000000000000000007e29","to":"0xE592427A0AEce92De3Edee1F18E0157C05861564","selector":"0x414bf389","chainId":1,"value":"0x0","nonce":42,"blockTimestamp":1748649600,"isSimulated":true},"enrichment":{"validityDeltaSec":300,"recipientIsContract":false,"effectiveRateVsOracleBps":8,"totalInputUsd":4800},"context":{"venue":"uniswap_v3","tokenIn":"USDC","tokenOut":"WETH","direction":{"kind":"exact_input","amountIn":"0x...","minAmountOut":"0x...","amountInUsd":4800,"amountOutUsd":4790},"recipient":"0xA1c4000000000000000000000000000000007e29","slippageBp":50,"priceImpactBp":12,"expectedAmountOut":"0x...","routeEstimatedOut":"0x...","gasEstimate":"0x..."},"expected":{"verdict":"ALLOW","permitMatch":true,"failed":[]}},{"id":"tx-market-expiry","label":"Market swap · 만료 임박","meta":{"from":"0xA1c4000000000000000000000000000000007e29","to":"0xE592427A0AEce92De3Edee1F18E0157C05861564","selector":"0x414bf389","chainId":1,"value":"0x0","nonce":43,"blockTimestamp":1748649600,"isSimulated":true},"enrichment":{"validityDeltaSec":18,"recipientIsContract":false,"effectiveRateVsOracleBps":140,"totalInputUsd":4800},"context":{"venue":"uniswap_v3","tokenIn":"USDC","tokenOut":"WETH","direction":{"kind":"exact_input","amountIn":"0x...","minAmountOut":"0x...","amountInUsd":4800,"amountOutUsd":4710},"recipient":"0xA1c4000000000000000000000000000000007e29","slippageBp":150,"priceImpactBp":60,"expectedAmountOut":"0x...","routeEstimatedOut":"0x...","gasEstimate":"0x..."},"expected":{"verdict":"DENY","permitMatch":false,"failed":["s2","s3"]}},{"id":"tx-send-to-contract","label":"Send to contract","meta":{"from":"0xA1c4000000000000000000000000000000007e29","to":"0xE592427A0AEce92De3Edee1F18E0157C05861564","selector":"0x414bf389","chainId":1,"value":"0x0","nonce":44,"blockTimestamp":1748649600,"isSimulated":true},"enrichment":{"validityDeltaSec":200,"recipientIsContract":true,"effectiveRateVsOracleBps":5,"totalInputUsd":4800},"context":{"venue":"uniswap_v3","tokenIn":"USDC","tokenOut":"WETH","direction":{"kind":"exact_input","amountIn":"0x...","minAmountOut":"0x...","amountInUsd":4800,"amountOutUsd":4790},"recipient":"0xBEEF000000000000000000000000000000001234","slippageBp":40,"priceImpactBp":10,"expectedAmountOut":"0x...","routeEstimatedOut":"0x...","gasEstimate":"0x..."},"expected":{"verdict":"DENY","permitMatch":false,"failed":["s1"]}}];

// ─── role groups (predicate visual identity) — §5 ──────────────────────────
// fill = domain palette (sage/slate/cyan family); icon distinguishes role.
var V7_ROLES = {
  numeric: { key:'numeric', en:'Number · limit', ko:'수량·한도', tone:'slate', icon:'hash'   },
  address: { key:'address', en:'Address',        ko:'주체·주소', tone:'cyan',  icon:'key'    },
  ref:     { key:'ref',     en:'Selection',      ko:'대상 선택', tone:'sage',  icon:'token'  },
  enum:    { key:'enum',    en:'Mode',           ko:'모드·열거', tone:'slate', icon:'switch' },
  auth:    { key:'auth',    en:'Auth · time',    ko:'서명·시간', tone:'cyan',  icon:'clock'  },
  misc:    { key:'misc',    en:'Other',          ko:'그 외',     tone:'slate', icon:'dot'    },
};
function v7RoleOf(param, fk){
  var g = V7_GLOSS[param];
  if (g && V7_ROLES[g.group]) return g.group;
  if (fk==='ref') return 'ref';
  if (fk==='collection') return 'auth';
  if (fk==='primitive.Bool') return 'enum';
  if (fk==='primitive.Long'||fk==='primitive.decimal') return 'numeric';
  if (/recipient|spender|from|to|delegatee|onBehalfOf|contract|address/i.test(param)) return 'address';
  return 'misc';
}
function v7IsLive(param){ var g=V7_GLOSS[param]; if(g) return !!g.derived; return /^enrichment\./.test(param); }
function v7Display(param, locale){
  var g=V7_GLOSS[param];
  if(g) return locale==='ko'? g.ko : g.en;
  var leaf=(param||'').split('.').pop();
  return leaf.replace(/([a-z0-9])([A-Z])/g,'$1 $2').replace(/^./,function(c){return c.toUpperCase();});
}

// ─── operators per fieldKind + symbols ─────────────────────────────────────
var V7_OPS = {
  'primitive.String':['eq','neq','in','notIn','startsWith','contains'],
  'primitive.Long':['eq','neq','lt','lte','gt','gte'],
  'primitive.decimal':['eq','neq','lt','lte','gt','gte'],
  'primitive.Bool':['isTrue','isFalse'],
  'ref':['eq','neq','in','notIn'],
  'collection':['contains','containsAny','containsAll','isEmpty','sizeEq','sizeGt','sizeLt'],
  'record':[],
};
var V7_OPSYM = { eq:'==', neq:'≠', lt:'<', lte:'≤', gt:'>', gte:'≥', in:'∈', notIn:'∉',
  startsWith:'starts', contains:'has', isTrue:'= 참', isFalse:'= 거짓',
  containsAny:'⊇any', containsAll:'⊇all', isEmpty:'empty', sizeEq:'#=', sizeGt:'#>', sizeLt:'#<' };
var V7_UNIT = {"context.amountUsd":{"en":"USD","ko":"USD"},"context.slippageBp":{"en":"bp","ko":"bp"},"context.priceImpactBp":{"en":"bp","ko":"bp"},"enrichment.validityDeltaSec":{"en":"sec","ko":"초"},"enrichment.totalInputUsd":{"en":"USD","ko":"USD"},"enrichment.effectiveRateVsOracleBps":{"en":"bp","ko":"bp"}};
// unit for a param in a locale (canonical block unit if no entry). sec→초 handled in glossary ko.
function v7Unit(param, locale){ var u=V7_UNIT[param]; if(!u) return ""; return locale==="en"? u.en : u.ko; }

// ─── id + node helpers ─────────────────────────────────────────────────────
var _v7n = 0; function v7Id(p){ _v7n++; return (p||'n')+'_'+Date.now().toString(36)+_v7n; }

// ─── nested-document baseline (golden samplePolicy) ────────────────────────
function v7Val(v){
  if(v && typeof v==='object') return v;
  if(typeof v==='string' && v[0]==='@') return {kind:'ref', text:v};
  if(typeof v==='number') return {kind:'num', text:String(v)};
  if(typeof v==='boolean') return {kind:'bool', text:String(v)};
  return {kind:'str', text:String(v==null?'':v)};
}
function v7BuildDoc(){
  var nodes={};
  function put(n){ nodes[n.id]=n; return n; }
  function pred(p, cfg){
    var unit=V7_UNIT[p]; var val=cfg.value!==undefined? v7Val(cfg.value): null;
    if(val && unit && val.kind==='num') val.unit=unit.en;
    return put({ id:v7Id('p'), type:'predicate', param:p, fieldKind:cfg.fk,
      op:cfg.op, value:val, absence:cfg.absence||(/^enrichment\./.test(p)?'treatAsFalse':null),
      parentId:cfg.parentId||null, x:0, y:0 });
  }
  function logic(op, parentId, extra){
    return put(Object.assign({ id:v7Id('L'), type:'logic', op:op, childIds:[], parentId:parentId||null, x:0, y:0 }, extra||{}));
  }

  // hat (permit · Amm::Swap)
  var hat = put({ id:'hat', type:'hat', effect:'permit', action:'Amm::Swap', childId:null, x:80, y:120 });
  // root AND (the when-tree)
  var root = logic('AND', 'hat'); hat.childId=root.id;

  // s1 — swap-and-send 배제 : NOT( AND( recipient≠from, recipientIsContract ) )
  var s1 = logic('NOT', root.id, { guardId:'s1', label:'swap-and-send 배제', enabled:true, userCopy:{ headline:'외부 컨트랙트로 빼돌리기 차단', plain:'수신자가 내 지갑이 아닌 컨트랙트면 막습니다' } });
  var s1and = logic('AND', s1.id);
  var s1a = pred('context.recipient', { fk:'primitive.String', op:'neq', value:'@meta.from', parentId:s1and.id });
  var s1b = pred('enrichment.recipientIsContract', { fk:'primitive.Bool', op:'isTrue', parentId:s1and.id });
  s1and.childIds=[s1a.id,s1b.id]; s1.childIds=[s1and.id];

  // s2 — 슬리피지 가드 : slippageBp < 100
  var s2 = pred('context.slippageBp', { fk:'primitive.Long', op:'lt', value:100, parentId:root.id });
  s2.guardId='s2'; s2.label='슬리피지 가드'; s2.enabled=true;
  s2.userCopy={ headline:'슬리피지 상한', plain:'슬리피지가 100bp를 넘지 않아야 합니다' };

  // s3 — 만료임박+고임팩트 배제 : NOT( AND( validityDeltaSec<30, priceImpactBp>50 ) )
  var s3 = logic('NOT', root.id, { guardId:'s3', label:'만료임박+고임팩트 배제', enabled:true, userCopy:{ headline:'만료 임박 + 프라이스 임팩트 과다 차단', plain:'마감 30초 안 남았는데 프라이스 임팩트가 50bp를 넘으면 막습니다' } });
  var s3and = logic('AND', s3.id);
  var s3a = pred('enrichment.validityDeltaSec', { fk:'primitive.Long', op:'lt', value:30, absence:'treatAsFalse', parentId:s3and.id });
  var s3b = pred('context.priceImpactBp', { fk:'primitive.Long', op:'gt', value:50, parentId:s3and.id });
  s3and.childIds=[s3a.id,s3b.id]; s3.childIds=[s3and.id];

  root.childIds=[s1.id,s2.id,s3.id];

  // unconnected drafts (canvas, excluded from compile)
  var d1 = pred('enrichment.effectiveRateVsOracleBps', { fk:'primitive.Long', op:'lt', value:100 });
  d1.float=true; d1.x=120; d1.y=720; d1.note='오라클 슬리피지 안전조건 후보';
  var d2 = pred('enrichment.totalInputUsd', { fk:'primitive.decimal', op:'lt', value:10000 });
  d2.float=true; d2.x=400; d2.y=720; d2.note='대형거래 한도 후보';

  return { nodes:nodes, hatId:'hat', rootId:root.id,
    drafts:[d1.id,d2.id], locale:'ko',
    policyName:'Swap baseline', action:'Amm::Swap', denyMessage:'swap baseline not satisfied',
    readingHeader:'이 Swap을 허용하려면 — 아래를 모두 만족해야 합니다',
    pan:{x:0,y:0}, zoom:1 };
}

// ─── evaluation engine ─────────────────────────────────────────────────────
function v7ReadPath(tx, path){
  if(!path) return undefined;
  var clean=path.replace(/^@/,''); var parts=clean.split('.'); var cur=tx[parts[0]];
  for(var i=1;i<parts.length;i++){ if(cur==null) return undefined; cur=cur[parts[i]]; }
  return cur;
}
function v7ResolveVal(tx, v){
  if(!v) return undefined;
  if(v.kind==='ref' || (typeof v.text==='string' && v.text[0]==='@')) return v7ReadPath(tx, v.text);
  if(v.kind==='num') return Number(v.text);
  if(v.kind==='bool') return v.text==='true';
  return v.text;
}
function v7Apply(op, l, r){
  switch(op){
    case 'eq': return l===r; case 'neq': return l!==r;
    case 'lt': return Number(l)<Number(r); case 'lte': return Number(l)<=Number(r);
    case 'gt': return Number(l)>Number(r); case 'gte': return Number(l)>=Number(r);
    case 'isTrue': return l===true; case 'isFalse': return l===false;
    case 'in': return Array.isArray(r)&&r.indexOf(l)>=0; case 'notIn': return Array.isArray(r)&&r.indexOf(l)<0;
    case 'startsWith': return typeof l==='string'&&l.indexOf(String(r))===0;
    case 'contains': return Array.isArray(l)?l.indexOf(r)>=0:(typeof l==='string'&&l.indexOf(String(r))>=0);
    case 'isEmpty': return Array.isArray(l)?l.length===0:(l==null||l==='');
    default: return false;
  }
}
function v7EvalPred(n, tx){
  var lhs=v7ReadPath(tx, n.param);
  var optional=/^enrichment\./.test(n.param);
  if((lhs===undefined||lhs===null) && optional){
    var a=n.absence||'treatAsFalse';
    if(a==='treatAsTrue') return true;
    if(a==='skip') return true;
    return false;
  }
  var noRhs=(n.op==='isTrue'||n.op==='isFalse'||n.op==='isEmpty');
  var rhs=noRhs?undefined:v7ResolveVal(tx, n.value);
  return v7Apply(n.op, lhs, rhs);
}
function v7EvalNode(doc, id, tx, truth){
  var n=doc.nodes[id]; if(!n) return true;
  var res;
  if(n.type==='predicate'){ res=v7EvalPred(n, tx); }
  else if(n.type==='hat'){ res=v7EvalNode(doc, n.childId, tx, truth); }
  else { // logic
    var kids=(n.childIds||[]).filter(function(c){ var k=doc.nodes[c]; return k && k.enabled!==false; });
    if(kids.length===0){ res = n.op!=='OR'; }
    else if(n.op==='NOT'){ res=!v7EvalNode(doc, kids[0], tx, truth); }
    else { // evaluate ALL children (no short-circuit) so truth is fully populated for display
      var rs=kids.map(function(c){ return v7EvalNode(doc,c,tx,truth); });
      res = n.op==='OR' ? rs.some(Boolean) : rs.every(Boolean);
    }
  }
  truth[id]=res; return res;
}
// verdict: when-tree(root AND) true → ALLOW else DENY(deny-by-default)
function v7Evaluate(doc, tx){
  var truth={};
  var root=doc.nodes[doc.rootId];
  var ok=v7EvalNode(doc, doc.hatId, tx, truth);
  var failed=[];
  if(root && root.childIds){
    root.childIds.forEach(function(cid){
      var c=doc.nodes[cid]; if(!c || c.enabled===false) return;
      if(truth[cid]===false) failed.push({ id:cid, guardId:c.guardId||cid, label:c.label||v7Display(c.param,doc.locale) });
    });
  }
  return { verdict: ok?'ALLOW':'DENY', permitMatch:ok, truth:truth, failed:failed };
}

// ─── Cedar serializer (permit, real paths) ─────────────────────────────────
function v7PredCedar(n){
  if(n.op==='isTrue') return n.param+' == true';
  if(n.op==='isFalse') return n.param+' == false';
  var sym={eq:'==',neq:'!=',lt:'<',lte:'<=',gt:'>',gte:'>='}[n.op]||n.op;
  var v=n.value||{}; var rhs;
  if(v.kind==='ref'||(v.text&&v.text[0]==='@')) rhs=String(v.text).replace(/^@/,'');
  else if(v.kind==='num') rhs=v.text; else rhs='"'+v.text+'"';
  var expr=n.param+' '+sym+' '+rhs;
  if(/^enrichment\./.test(n.param)){ var seg=n.param.split('.'); return '('+seg[0]+' has '+seg.slice(1).join('.')+' && '+expr+')'; }
  return expr;
}
function v7NodeCedar(doc, id){
  var n=doc.nodes[id]; if(!n) return 'true';
  if(n.type==='predicate') return v7PredCedar(n);
  var kids=(n.childIds||[]).filter(function(c){ var k=doc.nodes[c]; return k && k.enabled!==false; });
  if(n.op==='NOT') return '!('+(kids.length?v7NodeCedar(doc,kids[0]):'false')+')';
  if(kids.length===0) return n.op==='OR'?'false':'true';
  var join=n.op==='AND'?' && ':' || ';
  var parts=kids.map(function(c){ var t=v7NodeCedar(doc,c); var ck=doc.nodes[c]; if(ck&&ck.type==='logic'&&ck.op!=='NOT'&&ck.childIds.length>1) return '('+t+')'; return t; });
  return parts.join(join);
}
function v7ToCedar(doc){
  var lines=[]; var ln=0;
  function push(t,m){ ln++; lines.push(Object.assign({n:ln,text:t},m||{})); }
  push('@id("'+(doc.policyName||'swap_baseline').replace(/\s+/g,'_')+'")',{kind:'cmt'});
  push('permit (',{kind:'kw'});
  push('  principal,  // Wallet',{kind:'arg'});
  push('  action == Action::"'+(doc.action||'Amm::Swap')+'",',{kind:'arg'});
  push('  resource   // Protocol',{kind:'arg'});
  push(')',{kind:'punct'});
  push('when {',{kind:'kw'});
  var root=doc.nodes[doc.rootId];
  var guards=(root.childIds||[]).filter(function(c){ var k=doc.nodes[c]; return k && k.enabled!==false; });
  if(guards.length===0) push('  true  // (no safety conditions)',{kind:'cmt'});
  else guards.forEach(function(cid,i){
    var t=v7NodeCedar(doc,cid); var ck=doc.nodes[cid];
    var lab=ck.label?('  // '+ck.label):'';
    push('  '+(i>0?'&& ':'')+t+lab,{kind:'guard',guardId:ck.guardId||cid});
  });
  push('};',{kind:'kw'});
  var off=(root.childIds||[]).filter(function(c){var k=doc.nodes[c];return k&&k.enabled===false;}).length;
  if(off) push('// '+off+'개 가드 비활성 — 컴파일 제외',{kind:'cmt'});
  if(doc.drafts&&doc.drafts.length) push('// 미연결 '+doc.drafts.length+'개 — 컴파일 제외',{kind:'cmt'});
  return { lines:lines };
}

Object.assign(window, {
  V7_GLOSS, V7_SAMPLE_POLICY, V7_SAMPLE_TX, V7_ROLES, V7_OPS, V7_OPSYM, V7_UNIT, v7Unit,
  v7RoleOf, v7IsLive, v7Display, v7Id, v7Val,
  v7BuildDoc, v7ReadPath, v7ResolveVal, v7EvalNode, v7Evaluate, v7ToCedar, v7PredCedar,
});
