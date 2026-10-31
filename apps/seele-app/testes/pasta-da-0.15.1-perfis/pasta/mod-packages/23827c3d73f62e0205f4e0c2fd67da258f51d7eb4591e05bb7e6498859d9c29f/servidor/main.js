/* SEELE Perfis 1.2.2 — identidade e permissão vêm exclusivamente do contexto. */
(() => {
  'use strict';
  const LIMITS={avatar:10*1024*1024,banner:10*1024*1024}, CHUNK=6000, ASSET_PART=65536, MAX_PROFILES=128;
  const encodedLimit=slot=>4*Math.ceil(LIMITS[slot]/3)+32;
  const effects=['none','aurora','sparkle','pulse'];
  function fail(message) { throw new Error(message); }
  function text(value,max) { if (typeof value!=='string' || value.length>max) fail('Texto maior que o permitido.'); return value.trim(); }
  function person(value) { if (typeof value!=='string' || !/^[1-9][0-9]{0,19}$/.test(value)) fail('Pessoa inválida.'); return value; }
  function blank() { return {revision:0,displayName:'',pronouns:'',bio:'',status:'',accent:'#f2521f',effect:'none',avatar:null,banner:null}; }
  function validate(raw,old) {
    if (!raw || typeof raw!=='object') fail('Perfil inválido.');
    if (!/^#[0-9a-f]{6}$/i.test(raw.accent || '') || !effects.includes(raw.effect)) fail('Efeito ou cor inválidos.');
    return {...old,revision:old.revision+1,displayName:text(raw.displayName,40),pronouns:text(raw.pronouns,30),bio:text(raw.bio,280),status:text(raw.status,60),accent:raw.accent.toLowerCase(),effect:raw.effect};
  }
  // Leitura de imagens legadas em fragmentos; novos envios usam volume binário.
  function indexOf(raw){return raw?.startsWith('{')?JSON.parse(raw):null;}
  function sliceAsset(path,raw,offset){
    const index=indexOf(raw);if(!index)return raw?.slice(offset,offset+ASSET_PART)||null;
    let result='';const end=Math.min(index.length,offset+ASSET_PART);
    while(offset<end){
      const n=Math.floor(offset/CHUNK),part=arquivos.ler(path+'.'+n);
      if(!part)fail('Fragmento de imagem indisponível.');
      const piece=part.slice(offset%CHUNK,Math.min(part.length,offset%CHUNK+end-offset));
      if(!piece.length)fail('Fragmento de imagem incompleto.');
      result+=piece;offset+=piece.length;
    }
    return result;
  }
  globalThis.aoPedir=(contextJSON,requestJSON)=>{
    try {
      const ctx=JSON.parse(contextJSON),r=JSON.parse(requestJSON),me=person(ctx.person);
      const store=dados.profiles ? JSON.parse(dados.profiles) : {};
      // Delete superseded assets only on a later request, after metadata committed.
      // File effects cannot roll back with the database transaction.
      if(dados.garbage){
        const referenced=Object.values(store).flatMap(p=>[p.avatar,p.banner]);
        const pending=[];let budget=32;
        for(const entry of JSON.parse(dados.garbage)){
          const item=typeof entry==='string'?{path:entry,next:0}:entry;
          if(referenced.includes(item.path))continue;
          if(!budget){pending.push(item);continue;}
          if(item.path.startsWith('volume:')) {
            const nome=item.path.slice(7);
            if(volume.tamanho(nome)!==null&&!volume.apagar(nome))pending.push(item);
            budget--; continue;
          }
          const raw=arquivos.ler(item.path);if(raw===null)continue;
          const index=indexOf(raw);
          const count=index?.parts||0;
          while(item.next<count&&budget){const path=item.path+'.'+item.next;if(arquivos.ler(path)!==null&&!arquivos.apagar(path))break;item.next++;budget--;}
          if(item.next<count||!arquivos.apagar(item.path))pending.push(item);
          budget=Math.max(0,budget-1);
        }
        if(pending.length)dados.garbage=JSON.stringify(pending);else delete dados.garbage;
      }
      const retire=asset=>{if(asset)dados.garbage=JSON.stringify([...(dados.garbage?JSON.parse(dados.garbage):[]),asset]);};
      const own=store[me] || blank();
      if (r.op==='view') {
        const ids=Array.isArray(r.people) ? r.people : [me];
        if (ids.length>32) fail('Consulte até 32 perfis por vez.');
        const profiles={};
        for (const id of ids) { person(id); if (store[id]) profiles[id]=store[id]; }
        profiles[me]=own;
        return JSON.stringify({ok:true,me,profiles});
      }
      if (r.op==='asset') {
        const id=person(r.person),slot=r.slot;
        if (!['avatar','banner'].includes(slot)) fail('Imagem inválida.');
        const path=store[id]?.[slot];
        if (path?.startsWith('volume:')) {
          if (r.path && r.path !== path) fail('A imagem mudou. Abra o perfil novamente.');
          const nome=volume.servir(path.slice(7));
          if (!nome) fail('Imagem indisponível.');
          return JSON.stringify({ok:true,volume:nome});
        }
        const raw=path ? arquivos.ler(path) : null,index=indexOf(raw);
        const total=index?.length||raw?.length||0;
        if(r.offset!==undefined){
          if(!Number.isInteger(r.offset)||r.offset<0||r.offset>total)fail('Posição de imagem inválida.');
          if(r.path!==path)fail('A imagem mudou. Abra o perfil novamente.');
          const fim=Math.min(r.offset+ASSET_PART,total);
          return JSON.stringify({ok:true,path,total,image:sliceAsset(path,raw,r.offset),...(fim<total?{proximo:{offset:fim,path}}:{})});
        }
        // Small legacy assets retain their response shape; large assets require
        // pagination rather than exceeding SEELE's maximum reply-part count.
        // `proximo` é a continuação que o SEELE junta ao pedido seguinte sem
        // interpretar: ele não conhece a forma deste MOD, e não precisa.
        // `paged` fica para quem já lia a resposta antiga.
        if(total>ASSET_PART)return JSON.stringify({ok:true,path,total,image:sliceAsset(path,raw,0),paged:true,proximo:{offset:ASSET_PART,path}});
        return JSON.stringify({ok:true,image:sliceAsset(path,raw,0)});
      }
      // A forged owner/admin in r is irrelevant. Only ctx.person can mutate.
      if (r.person!==undefined && r.person!==me) fail('Você só pode editar o próprio perfil.');
      if (ctx.write!==true) fail('Você não tem permissão de escrita neste canal.');
      if (!store[me] && Object.keys(store).length>=MAX_PROFILES) fail('Este servidor atingiu o limite de 128 perfis.');
      if (r.op==='upload-finish') {
        const up=dados['upload-'+me] ? JSON.parse(dados['upload-'+me]) : null;
        if (!up || up.transport!=='volume' || up.token!==r.token) fail('Upload expirado ou fora de ordem.');
        const bytes=volume.tamanho(up.path.slice(7));
        if (bytes!==up.bytes) fail('Imagem incompleta. Escolha novamente.');
        const previous=own[up.slot];
        store[me]={...own,[up.slot]:up.path,revision:own.revision+1};
        dados.profiles=JSON.stringify(store); delete dados['upload-'+me]; retire(previous);
        return JSON.stringify({ok:true,finished:true,profile:store[me]});
      }
      if (r.op==='upload-start') {
        if (!['avatar','banner'].includes(r.slot)) fail('Imagem inválida.');
        if (r.transporte==='volume') {
          if(typeof volume==='undefined' || typeof volume.tamanho!=='function') fail('Atualize o SEELE de quem hospeda para enviar imagens por fluxo.');
          if(!Number.isInteger(r.bytes)||r.bytes<1||r.bytes>LIMITS[r.slot]) fail('Use uma imagem de até 10 MiB.');
          const token=String(mundo.agora())+'-'+Math.random().toString(36).slice(2,14);
          const path='images/'+me+'-'+token;
          if(!volume.esperar(token,path,['png','jpeg','webp','gif'],600)) fail('Não foi possível iniciar o envio.');
          const old=dados['upload-'+me] ? JSON.parse(dados['upload-'+me]) : null;
          dados['upload-'+me]=JSON.stringify({transport:'volume',token,path:'volume:'+path,slot:r.slot,bytes:r.bytes});
          if(old)retire(old.path);
          return JSON.stringify({ok:true,token});
        }
        if (!Number.isInteger(r.length) || r.length<1 || r.length>encodedLimit(r.slot)) fail('Use uma imagem de até 10 MiB.');
        const old=dados['upload-'+me] ? JSON.parse(dados['upload-'+me]) : null;
        const token=String(mundo.agora())+'-'+Math.random().toString(36).slice(2,14);
        const upload={token,path:'uploads/'+me+'-'+token+'.parts',slot:r.slot,total:r.length,length:0,next:0,started:mundo.agora()};
        dados['upload-'+me]=JSON.stringify(upload);
        if (old) retire(old.path);
        return JSON.stringify({ok:true,token});
      }
      if (r.op==='upload-part') {
        const up=dados['upload-'+me] ? JSON.parse(dados['upload-'+me]) : null;
        if (!up || up.token!==r.token || up.next!==r.index || mundo.agora()-up.started>600) fail('Upload expirado ou fora de ordem. Escolha a imagem novamente.');
        if (typeof r.part!=='string' || !r.part.length || r.part.length>CHUNK || up.length+r.part.length>up.total) fail('Fragmento inválido.');
        const finished=up.length+r.part.length===up.total;
        if(!finished&&r.part.length!==CHUNK)fail('Fragmento incompleto.');
        let payload=r.part;
        if(up.next===0){
          const header=/^data:image\/(png|jpeg|webp|gif);base64,/.exec(payload);
          const starts={png:'iVBORw0KGgo',jpeg:'/9j/',gif:'R0lGOD',webp:'UklGR'};
          if(!header||!payload.slice(header[0].length).startsWith(starts[header[1]]))fail('Use PNG, JPEG, WebP ou GIF válido.');
          up.header=header[0].length;payload=payload.slice(up.header);
        }
        if(!(finished?/^[A-Za-z0-9+/]*={0,2}$/:/^[A-Za-z0-9+/]+$/).test(payload))fail('Imagem inválida.');
        if (finished) {
          const length=up.total-up.header;
          if(length%4!==0)fail('Imagem inválida.');
          const bytes=length/4*3-(payload.endsWith('==')?2:payload.endsWith('=')?1:0);
          if(bytes>LIMITS[up.slot])fail('Imagem maior que o limite deste campo.');
        }
        const partPath=up.path+'.'+up.next;
        if (!arquivos.escrever(partPath,r.part)) fail('Falha ao gravar a imagem.');
        up.length+=r.part.length;up.next++;up.started=mundo.agora();
        if(!arquivos.escrever(up.path,JSON.stringify({format:1,parts:up.next,length:up.length}))){arquivos.apagar(partPath);fail('Falha ao gravar o índice da imagem.');}
        if (finished) {
          const previous=own[up.slot];
          store[me]={...own,[up.slot]:up.path,revision:own.revision+1};
          dados.profiles=JSON.stringify(store); delete dados['upload-'+me];
          retire(previous);
        } else dados['upload-'+me]=JSON.stringify(up);
        return JSON.stringify({ok:true,finished,revision:store[me]?.revision || 0});
      }
      if (r.revision!==own.revision) fail('Seu perfil mudou em outra janela. Recarregue antes de salvar.');
      if (r.op==='save') store[me]=validate(r.profile,own);
      else if (r.op==='clear-image') {
        if (!['avatar','banner'].includes(r.slot)) fail('Imagem inválida.');
        store[me]={...own,[r.slot]:null,revision:own.revision+1};
        retire(own[r.slot]);
      } else if (r.op==='reset') {
        store[me]={...blank(),revision:own.revision+1};
        for (const slot of ['avatar','banner']) retire(own[slot]);
      } else fail('Operação desconhecida.');
      dados.profiles=JSON.stringify(store);
      return JSON.stringify({ok:true,me,profile:store[me]});
    } catch(e) { return JSON.stringify({ok:false,error:e.message}); }
  };
})();
