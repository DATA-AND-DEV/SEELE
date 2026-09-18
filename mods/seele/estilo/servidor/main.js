/* SEELE Estilo 1.0.0 — API 2, sem dependências ou rede externa. */
(() => {
  'use strict';
  const defaults = { accent:'#a78bfa', background:'#10121b', panel:'#191d2b', text:'#f2f4ff', muted:'#b3bdd4', border:'#424b65', radius:12, density:'comfortable', font:'sans', glow:false };
  const colorKeys = ['accent','background','panel','text','muted','border'];
  function fail(message) { throw new Error(message); }
  function luminance(hex) {
    const rgb = [1,3,5].map(i => parseInt(hex.slice(i,i+2),16)/255).map(v => v<=0.04045 ? v/12.92 : ((v+0.055)/1.055)**2.4);
    return rgb[0]*0.2126+rgb[1]*0.7152+rgb[2]*0.0722;
  }
  function contrast(a,b) { const x=luminance(a),y=luminance(b); return (Math.max(x,y)+0.05)/(Math.min(x,y)+0.05); }
  function validate(raw) {
    if (!raw || typeof raw!=='object' || Array.isArray(raw)) fail('Tema inválido.');
    const out={};
    for (const key of colorKeys) { if (!/^#[0-9a-f]{6}$/i.test(raw[key] || '')) fail('Use cores hexadecimais de seis dígitos.'); out[key]=raw[key].toLowerCase(); }
    for (const foreground of ['text','muted','accent']) for (const background of ['background','panel']) {
      if (contrast(out[foreground],out[background]) < 4.5) fail('Contraste insuficiente: ajuste texto, texto secundário ou destaque sobre os fundos (mínimo 4,5:1).');
    }
    if (!Number.isInteger(raw.radius) || raw.radius<0 || raw.radius>24) fail('Arredondamento deve estar entre 0 e 24.');
    if (!['comfortable','compact'].includes(raw.density) || !['sans','mono'].includes(raw.font) || typeof raw.glow!=='boolean') fail('Opção de aparência inválida.');
    return {...out,radius:raw.radius,density:raw.density,font:raw.font,glow:raw.glow};
  }
  globalThis.aoPedir=(contextJSON,requestJSON)=>{
    try {
      const ctx=JSON.parse(contextJSON),r=JSON.parse(requestJSON);
      if (!ctx.person) fail('Sessão inválida.');
      let state=dados.theme ? JSON.parse(dados.theme) : { revision:0,theme:defaults,enabled:false };
      if (r.op!=='view') {
        if (ctx.admin!==true) fail('Somente o host ou administrador pode mudar o tema.');
        if (r.revision!==state.revision) fail('O tema mudou em outro computador. Recarregue antes de salvar.');
        if (r.op==='save') state={revision:state.revision+1,theme:validate(r.theme),enabled:true};
        else if (r.op==='reset') state={revision:state.revision+1,theme:defaults,enabled:false};
        else fail('Operação desconhecida.');
        dados.theme=JSON.stringify(state);
      }
      return JSON.stringify({ok:true,...state,canEdit:ctx.admin===true});
    } catch(e) { return JSON.stringify({ok:false,error:e.message}); }
  };
})();
