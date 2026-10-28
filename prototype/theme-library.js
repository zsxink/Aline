/* 内置主题目录、预览图片与 ZIP 主题包的原型交互。 */
(() => {
  'use strict';
  const $ = selector => document.querySelector(selector);
  const modes = [['light','白天'],['dark','夜间'],['sepia','护眼']];
  const themeDescriptions = {
    github:'沿用 GitHub Markdown 的清晰标题层级与蓝色链接，表格和代码块用轻量灰底分组。素白画布搭配中性边框，适合长文与技术文档阅读。',
    notion:'以柔和灰白和宽松留白突出正文，标题分层简洁，代码块和表格保持低对比表面。整体减少界面强调，适合笔记、知识整理和长时间阅读。',
    vue:'以 Vue 绿作为链接和交互强调色，白色画布搭配清晰标题层级与深色代码区。结构轻快、对比清楚，适合组件文档和开发指南。',
    feishu:'用飞书蓝做链接与交互强调，浅灰侧栏和细边框分出内容区域。表格、代码采用克制底色，适合协作说明、流程文档和日常笔记。'
  };
  const tokenMap = {bg:'--bg',surface:'--surface',fg:'--fg',muted:'--muted',border:'--border',accent:'--accent',code:'--code-bg'};
  const requiredColors=['bg','surface','sidebar','fg','muted','border','code','accent','syntax'];
  let catalog = [];
  let refreshSequence = 0;
  let activePreview = null;
  const escapeHtml = value => String(value).replace(/[&<>"']/g, char => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
  const customThemes = () => { try { const items=JSON.parse(localStorage.getItem('aline.customThemes')||'[]');return Array.isArray(items)?items:[]; } catch { return []; } };
  const report = (message, error=false) => { const status=$('#theme-library-status');if(status){status.textContent=message;status.classList.toggle('error',error);} };
  const accentFor = theme => {const color=theme.colors?.[document.documentElement.dataset.theme||'light']?.accent||theme.colors?.light?.accent;return /^#[0-9a-f]{6}$/i.test(color||'')?color:'#0969da';};
  function validateManifest(theme,id){
    if(theme.schemaVersion!==1||theme.id!==id||!/^[a-z0-9][a-z0-9-]{0,47}$/.test(theme.id||'')||typeof theme.name!=='string'||theme.name.length<1||theme.name.length>48||typeof theme.version!=='string'||!theme.version.trim())throw new Error(`主题清单无效：${id}`);
    const paths=new Set();
    for(const [mode] of modes){
      const colors=theme.colors?.[mode],path=theme.previews?.[mode];
      if(!colors||requiredColors.some(key=>!/^#[0-9a-f]{6}$/i.test(colors[key]||'')))throw new Error(`${id} 缺少 ${mode} 模式的有效颜色`);
      if(typeof path!=='string'||!path||path.startsWith('/')||/^[a-z]:/i.test(path)||path.includes('\\')||path.split('/').includes('..')||paths.has(path))throw new Error(`${id} 必须为三种模式提供不同的安全预览路径`);
      paths.add(path);
    }
    return theme;
  }
  async function decodePreview(bytes,extension){
    if(!validImage(bytes,extension))throw new Error('预览图片数据与扩展名不匹配');
    if(extension==='svg'){
      const xml=new DOMParser().parseFromString(new TextDecoder().decode(bytes),'image/svg+xml');
      if(xml.querySelector('parsererror')||xml.documentElement.localName!=='svg')throw new Error('SVG 预览图片无法解析');
      return;
    }
    if(typeof createImageBitmap==='function'){
      const mime={png:'image/png',jpg:'image/jpeg',jpeg:'image/jpeg',webp:'image/webp'}[extension];
      const bitmap=await createImageBitmap(new Blob([bytes],{type:mime}));bitmap.close();
    }
  }

  function renderCards(){
    const gallery=$('#theme-gallery');if(!gallery)return;
    const selected=document.documentElement.dataset.palette||'github';
    const currentMode=document.documentElement.dataset.theme||'light';
    gallery.innerHTML=catalog.map(theme=>{
      const id=escapeHtml(theme.id),name=escapeHtml(theme.name),accent=escapeHtml(accentFor(theme));
      const src=theme.previewData?.[currentMode]||`./themes/${encodeURIComponent(theme.id)}/${theme.previews[currentMode].split('/').map(encodeURIComponent).join('/')}`;
      const label=modes.find(([mode])=>mode===currentMode)?.[1]||'白天';
      const description=theme.description||themeDescriptions[theme.id]||`${theme.name}主题为 Markdown 阅读与编辑提供一套协调的界面配色。`;
      const origin=theme.builtin?'<small class="theme-origin">内置</small>':'<small class="theme-origin imported">已导入</small>';
      return `<article class="theme-family-card${theme.id===selected?' selected':''}" data-palette="${id}" style="--theme-card-accent:${accent}"><div class="theme-family-heading"><button type="button" class="theme-family-select" data-palette="${id}" aria-pressed="${theme.id===selected}"><span class="theme-accent-swatch" aria-hidden="true"></span><strong>${name}</strong>${theme.id==='github'&&theme.builtin?'<small>默认</small>':''}${origin}</button></div><div class="theme-preview-row"><button type="button" class="theme-preview-open" data-theme-id="${id}" data-preview-mode="${currentMode}" aria-label="预览 ${name} 主题，当前为${label}模式"><img src="${escapeHtml(src)}" alt="${name} ${label}模式界面预览"><span class="theme-preview-overlay"><span>预览</span></span></button><p class="theme-description" tabindex="0" title="${escapeHtml(description)}" data-full="${escapeHtml(description)}" aria-label="${escapeHtml(description)}">${escapeHtml(description)}</p></div></article>`;
    }).join('');
  }

  function syncCards(){
    const selected=document.documentElement.dataset.palette||'github';
    document.querySelectorAll('.theme-family-card').forEach(card=>{
      const active=card.dataset.palette===selected;card.classList.toggle('selected',active);
      card.querySelector('.theme-family-select')?.setAttribute('aria-pressed',String(active));
    });
  }

  async function refreshThemeGallery(){
    const gallery=$('#theme-gallery');if(!gallery)return;
    const sequence=++refreshSequence;report('正在读取主题文件夹…');
    try{
      let entries=[];
      try{
        const listing=await fetch(`./themes/?refresh=${Date.now()}`,{cache:'no-store'});
        const html=listing.ok?await listing.text():'';
        if(html){const page=new DOMParser().parseFromString(html,'text/html');entries=[...page.querySelectorAll('a[href]')].map(link=>decodeURIComponent(link.getAttribute('href')||'')).map(href=>href.replace(/^\.\//,'')).filter(href=>/^[a-z0-9][a-z0-9-]{0,47}\/$/.test(href)).map(folder=>({id:folder.slice(0,-1),manifest:`${folder}theme.json`}));}
      }catch{}
      if(!entries.length){const indexResponse=await fetch(`./themes/index.json?refresh=${Date.now()}`,{cache:'no-store'});if(!indexResponse.ok)throw new Error('无法读取主题文件夹或内置主题索引');const index=await indexResponse.json();entries=index.themes;}
      const loaded=await Promise.all(entries.map(async entry=>{
        const manifestPath=entry.manifest||`${entry.id}/theme.json`;
        if(manifestPath.startsWith('/')||/^[a-z]:/i.test(manifestPath)||manifestPath.includes('\\')||manifestPath.split('/').includes('..'))throw new Error(`主题清单路径无效：${entry.id}`);
        const manifestUrl=`./themes/${manifestPath.split('/').map(encodeURIComponent).join('/')}`;
        const result=await fetch(`${manifestUrl}?refresh=${Date.now()}`,{cache:'no-store'});
        if(!result.ok)throw new Error(`主题包清单读取失败：${entry.id}`);
        const theme=validateManifest(await result.json(),entry.id);
        for(const [mode] of modes){
          const relative=theme.previews[mode];
          const imageUrl=`./themes/${entry.id}/${relative.split('/').map(encodeURIComponent).join('/')}?refresh=${Date.now()}`;
          const imageResult=await fetch(imageUrl,{cache:'no-store'});if(!imageResult.ok)throw new Error(`${entry.id} 缺少 ${mode} 预览图`);
          const image=new Uint8Array(await imageResult.arrayBuffer());if(image.length>1024*1024)throw new Error(`${entry.id} 的预览图超过 1 MB`);
          await decodePreview(image,relative.split('.').pop().toLowerCase());
        }
        return {...theme,builtin:theme.builtin===true};
      }));
      if(sequence!==refreshSequence)return;
      const installed=customThemes();
      const ids=new Set(loaded.map(theme=>theme.id));
      const preferredOrder=['github','notion','vue','feishu'];
      catalog=[...loaded,...installed.filter(theme=>!ids.has(theme.id))].sort((a,b)=>{
        const rankA=preferredOrder.indexOf(a.id),rankB=preferredOrder.indexOf(b.id);
        if(rankA>=0||rankB>=0)return (rankA<0?preferredOrder.length:rankA)-(rankB<0?preferredOrder.length:rankB);
        return a.name.localeCompare(b.name,'zh-CN');
      });
      renderCards();syncCards();
      report(`已加载 ${catalog.length} 个主题`);
    }catch(error){
      if(sequence!==refreshSequence)return;
      const installed=customThemes();
      if(!catalog.length)catalog=installed;
      renderCards();syncCards();
      report(`刷新失败：${error.message}`,true);
    }
  }

  window.getThemeCatalog=()=>catalog.map(({id,name})=>({id,name}));
  window.hasThemeFamily=id=>catalog.some(theme=>theme.id===id);
  window.onThemePaletteChanged=(id,mode)=>{
    const root=document.documentElement;
    const theme=catalog.find(item=>item.id===id);
    Object.values(tokenMap).forEach(token=>root.style.removeProperty(token));
    updateCurrentModePreviews();
    syncCards();
    if(!theme||theme.builtin)return;
    const colors=theme.colors?.[mode];if(!colors)return;
    Object.entries(tokenMap).forEach(([key,token])=>{if(/^#[0-9a-f]{6}$/i.test(colors[key]||''))root.style.setProperty(token,colors[key]);});
    if(root.style.getPropertyValue('--accent'))root.style.setProperty('--accent-soft',`${colors.accent}1c`);
  };
  window.refreshThemeGallery=refreshThemeGallery;

  function updateCurrentModePreviews(){
    const mode=document.documentElement.dataset.theme||'light';
    for(const button of document.querySelectorAll('.theme-preview-open')){
      const theme=catalog.find(item=>item.id===button.dataset.themeId);if(!theme)continue;
      const src=theme.previewData?.[mode]||`./themes/${encodeURIComponent(theme.id)}/${theme.previews[mode].split('/').map(encodeURIComponent).join('/')}`;
      const label=modes.find(([key])=>key===mode)?.[1]||'白天';
      button.dataset.previewMode=mode;button.setAttribute('aria-label',`预览 ${theme.name} 主题，当前为${label}模式`);
      const image=button.querySelector('img');if(image){image.src=src;image.alt=`${theme.name} ${label}模式界面预览`;}
    }
  }
  function closeDialog(){document.querySelectorAll('.theme-library-backdrop').forEach(dialog=>dialog.remove());activePreview=null;}
  function previewSource(theme,mode){return theme.previewData?.[mode]||`./themes/${encodeURIComponent(theme.id)}/${theme.previews[mode].split('/').map(encodeURIComponent).join('/')}`;}
  function renderPreview(){
    const dialog=$('.preview-backdrop'),theme=catalog.find(item=>item.id===activePreview?.themeId);if(!dialog||!theme)return;
    const [,label]=modes.find(([mode])=>mode===activePreview.mode)||modes[0];
    dialog.querySelector('.theme-preview-dialog').setAttribute('aria-label',`${theme.name} ${label}模式主题预览`);
    dialog.querySelector('h2').textContent=`${theme.name} · ${label}`;
    const image=dialog.querySelector('.theme-preview-image');image.src=previewSource(theme,activePreview.mode);image.alt=`${theme.name} ${label}模式主题预览`;
    dialog.querySelector('.theme-preview-mode').textContent=label;
  }
  function showPreview(themeId,mode){
    closeDialog();activePreview={themeId,mode};
    const dialog=document.createElement('div');dialog.className='theme-library-backdrop preview-backdrop';
    const theme=catalog.find(item=>item.id===themeId);const imageSrc=theme?previewSource(theme,mode):'';
    dialog.innerHTML=`<section class="theme-preview-dialog" role="dialog" aria-modal="true"><button class="theme-library-close" type="button" aria-label="关闭预览"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4 4 8 8M12 4l-8 8"/></svg></button><h2></h2><div class="theme-preview-stage"><button type="button" class="theme-preview-step previous" aria-label="切换到上一种模式"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="m10 3-5 5 5 5"/></svg></button><img class="theme-preview-image" src="${escapeHtml(imageSrc)}" alt=""><button type="button" class="theme-preview-step next" aria-label="切换到下一种模式"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="m6 3 5 5-5 5"/></svg></button></div><div class="theme-preview-mode"></div></section>`;
    document.body.append(dialog);renderPreview();
  }
  function movePreview(step){if(!activePreview)return;const index=modes.findIndex(([mode])=>mode===activePreview.mode);activePreview.mode=modes[(index+step+modes.length)%modes.length][0];renderPreview();}

  function dataUrl(bytes,mime){
    let binary='';for(let i=0;i<bytes.length;i+=0x8000)binary+=String.fromCharCode(...bytes.subarray(i,Math.min(i+0x8000,bytes.length)));
    return `data:${mime};base64,${btoa(binary)}`;
  }
  async function extractEntry(bytes,entry){
    const view=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength),offset=entry.localOffset;
    if(view.getUint32(offset,true)!==0x04034b50)throw new Error(`ZIP 数据损坏：${entry.name}`);
    const nameLength=view.getUint16(offset+26,true),extraLength=view.getUint16(offset+28,true),start=offset+30+nameLength+extraLength;
    const compressed=bytes.subarray(start,start+entry.compressedSize);
    if(entry.method===0)return compressed;
    if(entry.method!==8||typeof DecompressionStream==='undefined')throw new Error('主题 ZIP 使用了当前原型不支持的压缩方式');
    const stream=new Blob([compressed]).stream().pipeThrough(new DecompressionStream('deflate-raw'));
    return new Uint8Array(await new Response(stream).arrayBuffer());
  }
  function zipEntries(bytes){
    const view=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength);let end=-1;
    for(let offset=bytes.length-22;offset>=Math.max(0,bytes.length-65557);offset--){if(view.getUint32(offset,true)===0x06054b50){end=offset;break;}}
    if(end<0)throw new Error('文件不是有效 ZIP，或 ZIP 目录已损坏');
    const count=view.getUint16(end+10,true),directoryOffset=view.getUint32(end+16,true);if(count===0xffff||directoryOffset===0xffffffff)throw new Error('暂不支持 ZIP64 主题包');
    const entries=new Map();let offset=directoryOffset,total=0;
    for(let n=0;n<count;n++){
      if(view.getUint32(offset,true)!==0x02014b50)throw new Error('ZIP 中央目录格式错误');
      const method=view.getUint16(offset+10,true),compressedSize=view.getUint32(offset+20,true),size=view.getUint32(offset+24,true),nameLength=view.getUint16(offset+28,true),extraLength=view.getUint16(offset+30,true),commentLength=view.getUint16(offset+32,true),localOffset=view.getUint32(offset+42,true);
      const name=new TextDecoder().decode(bytes.subarray(offset+46,offset+46+nameLength));
      if(name.startsWith('/')||/^[a-z]:/i.test(name)||name.includes('\\')||name.split('/').includes('..'))throw new Error('主题包包含越界路径');
      if(!name.endsWith('/')){if(entries.has(name))throw new Error(`ZIP 文件重复：${name}`);entries.set(name,{name,method,compressedSize,size,localOffset});total+=size;}
      offset+=46+nameLength+extraLength+commentLength;
    }
    if(total>5*1024*1024)throw new Error('主题包解压后超过 5 MB');
    return entries;
  }
  function validImage(bytes,extension){
    if(extension==='svg'){const start=new TextDecoder().decode(bytes.subarray(0,256)).trimStart().toLowerCase();return start.startsWith('<svg')||start.startsWith('<?xml');}
    if(extension==='png')return bytes[0]===137&&bytes[1]===80&&bytes[2]===78&&bytes[3]===71;
    if(extension==='jpg'||extension==='jpeg')return bytes[0]===255&&bytes[1]===216;
    if(extension==='webp')return new TextDecoder().decode(bytes.subarray(0,4))==='RIFF'&&new TextDecoder().decode(bytes.subarray(8,12))==='WEBP';
    return false;
  }
  async function validateThemeZip(file){
    if(!/\.zip$/i.test(file.name))throw new Error('请选择 .zip 格式的主题包');
    if(file.size>5*1024*1024)throw new Error('主题 ZIP 大小不能超过 5 MB');
    const bytes=new Uint8Array(await file.arrayBuffer()),entries=zipEntries(bytes);
    const manifestEntry=[...entries.values()].find(entry=>/(^|\/)theme\.json$/i.test(entry.name));
    if(!manifestEntry)throw new Error('主题包缺少 theme.json');
    const root=manifestEntry.name==='theme.json'?'':manifestEntry.name.slice(0,-'theme.json'.length);
    if(root&&root.split('/').filter(Boolean).length!==1)throw new Error('theme.json 只能放在 ZIP 根目录或单层主题目录中');
    const manifestBytes=await extractEntry(bytes,manifestEntry),manifest=JSON.parse(new TextDecoder().decode(manifestBytes));
    if(root&&root.slice(0,-1)!==manifest.id)throw new Error('ZIP 顶层主题目录名必须与主题 id 一致');
    if(manifest.schemaVersion!==1)throw new Error('主题包 schemaVersion 必须为 1');
    if(!/^[a-z0-9][a-z0-9-]{0,47}$/.test(manifest.id||''))throw new Error('主题 id 只能使用小写字母、数字和连字符');
    if(!manifest.name||manifest.name.length>48||!manifest.version)throw new Error('主题包需要有效的 name 和 version');
    if(catalog.some(theme=>theme.id===manifest.id))throw new Error(`主题 id 已存在：${manifest.id}`);
    const previewData={},previewPaths=new Set();
    for(const [mode] of modes){
      const relative=manifest.previews?.[mode];if(!relative||relative.startsWith('/')||relative.includes('\\')||relative.split('/').includes('..'))throw new Error(`缺少 ${mode} 模式预览图路径`);
      if(previewPaths.has(relative))throw new Error('白天、夜间、护眼必须分别提供三张预览图');previewPaths.add(relative);
      const path=`${root}${relative}`;const entry=entries.get(path);if(!entry)throw new Error(`主题包缺少预览图：${relative}`);
      if(entry.size>1024*1024)throw new Error(`单张预览图不能超过 1 MB：${relative}`);
      const image=await extractEntry(bytes,entry),extension=relative.split('.').pop().toLowerCase();
      try{await decodePreview(image,extension);}catch{throw new Error(`预览图格式无效：${relative}`);}
      const mime={svg:'image/svg+xml',png:'image/png',jpg:'image/jpeg',jpeg:'image/jpeg',webp:'image/webp'}[extension];
      previewData[mode]=dataUrl(image,mime);
    }
    validateManifest(manifest,manifest.id);
    return {...manifest,builtin:false,previewData};
  }

  document.addEventListener('click',event=>{
    const action=event.target.closest?.('[data-theme-action]');
    if(action){
      if(action.dataset.themeAction==='open-folder')window.open('./themes/','_blank','noopener');
      else if(action.dataset.themeAction==='import')$('#theme-zip-input')?.click();
      else if(action.dataset.themeAction==='refresh')refreshThemeGallery();
      return;
    }
    const selector=event.target.closest?.('.theme-family-select');if(selector){window.setPaletteFamily?.(selector.dataset.palette);syncCards();return;}
    const preview=event.target.closest?.('.theme-preview-open');if(preview){showPreview(preview.dataset.themeId,document.documentElement.dataset.theme||'light');return;}
    if(event.target.closest?.('.theme-preview-step.previous')){movePreview(-1);return;}
    if(event.target.closest?.('.theme-preview-step.next')){movePreview(1);return;}
    if(event.target.closest('.theme-library-close,.theme-library-primary')||event.target.classList?.contains('theme-library-backdrop'))closeDialog();
  });
  document.addEventListener('change',async event=>{
    if(event.target.id!=='theme-zip-input')return;
    const file=event.target.files?.[0];event.target.value='';if(!file)return;
    report(`正在检查 ${file.name}…`);
    try{
      const theme=await validateThemeZip(file);const installed=customThemes();installed.push(theme);
      try{localStorage.setItem('aline.customThemes',JSON.stringify(installed));}catch{throw new Error('浏览器主题存储空间不足，无法保存此主题包');}
      await refreshThemeGallery();report(`“${theme.name}”已通过检查并加入主题列表（原型存储）`);
    }catch(error){report(`导入失败：${error.message}`,true);}
  });
  document.addEventListener('click',event=>{
    if(event.target.closest?.('#settings-nav button[data-page="外观"],#settings-open,#status-settings'))setTimeout(()=>{if($('#theme-gallery'))refreshThemeGallery();},0);
  });
  document.addEventListener('keydown',event=>{if(!activePreview){if(event.key==='Escape')closeDialog();return;}if(event.key==='ArrowLeft'){movePreview(-1);event.preventDefault();}else if(event.key==='ArrowRight'){movePreview(1);event.preventDefault();}else if(event.key==='Escape')closeDialog();});
  if($('#theme-gallery'))refreshThemeGallery();
})();
