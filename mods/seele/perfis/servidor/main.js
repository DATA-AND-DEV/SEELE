/* SEELE Perfis 1.0.0 — identidade e permissão vêm exclusivamente do contexto. */
(() => {
  'use strict';
  const MAX_IMAGE=350000, CHUNK=6000, MAX_PROFILES=128;
  const effects=['none','aurora','sparkle','pulse'];
  function fail(message) { throw new Error(message); }
  function text(value,max) { if (typeof value!=='string' || value.length>max) fail('Texto maior que o permitido.'); return value.trim(); }
  function person(value) { if (typeof value!=='string' || !/^[1-9][0-9]{0,19}$/.test(value)) fail('Pessoa inválida.'); return value; }
  function blank() { return {revision:0,displayName:'',pronouns:'',bio:'',status:'',accent:'#a78bfa',effect:'none',avatar:null,banner:null}; }
  function validate(raw,old) {
    if (!raw || typeof raw!=='object') fail('Perfil inválido.');
    if (!/^#[0-9a-f]{6}$/i.test(raw.accent || '') || !effects.includes(raw.effect)) fail('Efeito ou cor inválidos.');
    return {...old,revision:old.revision+1,displayName:text(raw.displayName,40),pronouns:text(raw.pronouns,30),bio:text(raw.bio,280),status:text(raw.status,60),accent:raw.accent.toLowerCase(),effect:raw.effect};
  }
  function validImage(image) {
    const match=/^data:image\/(png|jpeg|webp|gif);base64,([A-Za-z0-9+/]+={0,2})$/.exec(image);
    if (!match || match[2].length%4!==0) return false;
    const starts={png:'iVBORw0KGgo',jpeg:'/9j/',gif:'R0lGOD',webp:'UklGR'};
    return match[2].startsWith(starts[match[1]]);
  }
  globalThis.aoPedir=(contextJSON,requestJSON)=>{
    try {
      const ctx=JSON.parse(contextJSON),r=JSON.parse(requestJSON),me=person(ctx.person);
      const store=dados.profiles ? JSON.parse(dados.profiles) : {};
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
        return JSON.stringify({ok:true,image:path ? arquivos.ler(path) : null});
      }
      // A forged owner/admin in r is irrelevant. Only ctx.person can mutate.
      if (r.person!==undefined && r.person!==me) fail('Você só pode editar o próprio perfil.');
      if (ctx.write!==true) fail('Você não tem permissão de escrita neste canal.');
      if (!store[me] && Object.keys(store).length>=MAX_PROFILES) fail('Este servidor atingiu o limite de 128 perfis.');
      if (r.op==='upload-start') {
        if (!['avatar','banner'].includes(r.slot)) fail('Imagem inválida.');
        if (!Number.isInteger(r.length) || r.length<1 || r.length>MAX_IMAGE) fail('Imagem muito grande: use um arquivo de até 256 KiB.');
        const old=dados['upload-'+me] ? JSON.parse(dados['upload-'+me]) : null;
        const token=String(mundo.agora())+'-'+Math.random().toString(36).slice(2,14);
        const upload={token,path:'uploads/'+me+'-'+token+'.txt',slot:r.slot,total:r.length,length:0,next:0,started:mundo.agora()};
        dados['upload-'+me]=JSON.stringify(upload);
        if (old) arquivos.apagar(old.path);
        return JSON.stringify({ok:true,token});
      }
      if (r.op==='upload-part') {
        const up=dados['upload-'+me] ? JSON.parse(dados['upload-'+me]) : null;
        if (!up || up.token!==r.token || up.next!==r.index || mundo.agora()-up.started>600) fail('Upload expirado ou fora de ordem. Escolha a imagem novamente.');
        if (typeof r.part!=='string' || !r.part.length || r.part.length>CHUNK || up.length+r.part.length>up.total) fail('Fragmento inválido.');
        const prefix=up.length ? arquivos.ler(up.path) : '';
        if (prefix===null || prefix.length<up.length) fail('Não foi possível ler o upload.');
        const image=prefix.slice(0,up.length)+r.part;
        const finished=image.length===up.total;
        if (finished && !validImage(image)) fail('Use PNG, JPEG, WebP ou GIF válido.');
        if (!arquivos.escrever(up.path,image)) fail('Falha ao gravar a imagem.');
        up.length=image.length; up.next++;
        if (finished) {
          const previous=own[up.slot];
          store[me]={...own,[up.slot]:up.path,revision:own.revision+1};
          dados.profiles=JSON.stringify(store); delete dados['upload-'+me];
          if (previous) arquivos.apagar(previous);
        } else dados['upload-'+me]=JSON.stringify(up);
        return JSON.stringify({ok:true,finished,revision:store[me]?.revision || 0});
      }
      if (r.revision!==own.revision) fail('Seu perfil mudou em outra janela. Recarregue antes de salvar.');
      if (r.op==='save') store[me]=validate(r.profile,own);
      else if (r.op==='clear-image') {
        if (!['avatar','banner'].includes(r.slot)) fail('Imagem inválida.');
        store[me]={...own,[r.slot]:null,revision:own.revision+1};
        if (own[r.slot]) arquivos.apagar(own[r.slot]);
      } else if (r.op==='reset') {
        store[me]={...blank(),revision:own.revision+1};
        for (const slot of ['avatar','banner']) if (own[slot]) arquivos.apagar(own[slot]);
      } else fail('Operação desconhecida.');
      dados.profiles=JSON.stringify(store);
      return JSON.stringify({ok:true,me,profile:store[me]});
    } catch(e) { return JSON.stringify({ok:false,error:e.message}); }
  };
})();
