/**
 * Static index for the global search: labels are existing locale keys and targets are stable DOM anchors.
 * Search only reads; navigation is the tab band's own switch (no setting is written, nothing is submitted).
 */
export type SearchView = 'studio' | 'downloads' | 'comfy' | 'backend' | 'logs';
export type SearchStudioTab = 'generate' | 'forge' | 'chat' | 'tve' | 'monitor' | 'map';
export type SearchPanel = 'startup' | 'llm' | 'auth';

export interface SearchEntry {
  view: SearchView;
  studioTab?: SearchStudioTab;
  panel?: SearchPanel;
  libraryView?: 'catalog' | 'installed';
  /** Beginner page that must be selected before the target exists. */
  beginnerTab?: 'generate' | 'models' | 'collection';
  /** Studio bottom-dock tab that must be selected before the target exists. */
  dockTab?: 'options' | 'post' | 'loras' | 'presets' | 'wildcards' | 'media' | 'models';
  /** Breadcrumb tab → section → control, all locale keys. */
  tab: string;
  section: string;
  label: string;
  target: string;
  /** English synonyms kept as data, so a hit works regardless of the display language. */
  aliases?: readonly string[];
  /** Locale key explaining why the control does not apply right now. */
  reason?: string;
}

const studio: SearchView = 'studio';

export const searchIndex: readonly SearchEntry[] = [
  // Beginner experience
  { view: studio, studioTab: 'generate', beginnerTab: 'generate', tab: 'beginner.navigation', section: 'beginner.generate', label: 'studio.layout.prompt', target: '[data-beginner-prompt-resize]', aliases: ['prompt height', 'resize', 'preview divider'], reason: 'beginner.switchHint' },
  { view: studio, studioTab: 'generate', beginnerTab: 'generate', tab: 'beginner.navigation', section: 'beginner.generate', label: 'studio.layout.promptCollapse', target: '[data-pane-toggle="prompt"]', aliases: ['expand prompt', 'hide prompt', 'collapse'], reason: 'beginner.switchHint' },
  { view: studio, studioTab: 'generate', beginnerTab: 'generate', tab: 'beginner.navigation', section: 'beginner.generate', label: 'beginner.generate', target: '[data-beginner-tab="generate"]', aliases: ['create', 'prompt', 'generate'] },
  { view: studio, studioTab: 'generate', beginnerTab: 'models', tab: 'beginner.navigation', section: 'beginner.models', label: 'beginner.models', target: '[data-beginner-tab="models"]', aliases: ['model library', 'discover models'] },
  { view: studio, studioTab: 'generate', beginnerTab: 'collection', tab: 'beginner.navigation', section: 'beginner.collection', label: 'beginner.collection', target: '[data-beginner-tab="collection"]', aliases: ['gallery', 'my creations'] },
  { view: studio, studioTab: 'generate', beginnerTab: 'generate', tab: 'beginner.navigation', section: 'beginner.generate', label: 'studio.run.resolution.aspect', target: '[data-beginner-aspect]', aliases: ['format', 'ratio', 'portrait', 'landscape'] },
  { view: studio, studioTab: 'generate', beginnerTab: 'generate', tab: 'beginner.navigation', section: 'beginner.generate', label: 'studio.run.resolution.title', target: '[data-beginner-resolution]', aliases: ['quality', 'size', 'megapixels'] },
  { view: studio, studioTab: 'generate', beginnerTab: 'generate', tab: 'beginner.navigation', section: 'settings.button', label: 'settings.button', target: '.beginner-backend', aliases: ['engine', 'backend setup'] },
  { view: studio, studioTab: 'generate', tab: 'beginner.navigation', section: 'beginner.experience', label: 'beginner.experience', target: '.experience-switch', aliases: ['beginner', 'advanced', 'mode'] },
  { view: 'backend', tab: 'settings.button', section: 'studio.quick.title', label: 'theme.ui.refresh', target: '[data-theme-action="refresh"]', aliases: ['reload themes', 'theme packs'], reason: 'theme.ui.chooseEnvironment' },
  { view: 'backend', tab: 'settings.button', section: 'studio.quick.title', label: 'theme.ui.editThemes', target: '[data-theme-action="edit"]', aliases: ['theme editor', 'new theme', 'custom theme'], reason: 'theme.ui.chooseEnvironment' },
  // Studio · settings
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.settings', label: 'studio.run.pipeline', target: '[data-pipeline]', aliases: ['mode', 'task'] },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.settings', label: 'studio.run.checkpoint', target: '[data-model-picker]', aliases: ['model'], reason: 'search.reason.models' },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.settings', label: 'studio.run.resolution.title', target: '[data-resolution]', aliases: ['size'], reason: 'search.reason.models' },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.settings', label: 'studio.run.width', target: '[data-num="width"]', aliases: ['resolution', 'size'], reason: 'search.reason.models' },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.settings', label: 'studio.run.height', target: '[data-num="height"]', aliases: ['resolution', 'size'], reason: 'search.reason.models' },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.settings', label: 'studio.run.steps', target: '[data-num="steps"]', aliases: ['sampling'] },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.settings', label: 'studio.run.cfg', target: '[data-num="cfg"]', aliases: ['guidance'] },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.settings', label: 'studio.recipe.sampler', target: '[data-pick="studio.recipe.sampler"]' },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.settings', label: 'studio.recipe.scheduler', target: '[data-pick="studio.recipe.scheduler"]' },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.settings', label: 'studio.run.seed', target: '[data-seed]' },
  // Music authoring
  {view:studio,studioTab:'generate',tab:'setup.tab.studio',section:'studio.run.pipe.t2m',label:'studio.music.ace.shift',target:'[data-ace-setting="shift"]',aliases:['ACE-Step','music','shift'],reason:'studio.run.pipe.noRecipe'},
  {view:studio,studioTab:'generate',tab:'setup.tab.studio',section:'studio.run.pipe.t2m',label:'studio.music.ace.lyricsStrength',target:'[data-ace-setting="lyricsStrength"]',aliases:['ACE-Step','music','lyricsStrength'],reason:'studio.run.pipe.noRecipe'},
  {view:studio,studioTab:'generate',tab:'setup.tab.studio',section:'studio.run.pipe.t2m',label:'studio.music.ace.bpm',target:'[data-ace-setting="bpm"]',aliases:['ACE-Step','music','bpm'],reason:'studio.run.pipe.noRecipe'},
  {view:studio,studioTab:'generate',tab:'setup.tab.studio',section:'studio.run.pipe.t2m',label:'studio.music.ace.timeSignature',target:'[data-ace-setting="timeSignature"]',aliases:['ACE-Step','music','timeSignature'],reason:'studio.run.pipe.noRecipe'},
  {view:studio,studioTab:'generate',tab:'setup.tab.studio',section:'studio.run.pipe.t2m',label:'studio.music.ace.language',target:'[data-ace-setting="language"]',aliases:['ACE-Step','music','language'],reason:'studio.run.pipe.noRecipe'},
  {view:studio,studioTab:'generate',tab:'setup.tab.studio',section:'studio.run.pipe.t2m',label:'studio.music.ace.keyScale',target:'[data-ace-setting="keyScale"]',aliases:['ACE-Step','music','keyScale'],reason:'studio.run.pipe.noRecipe'},
  {view:studio,studioTab:'generate',tab:'setup.tab.studio',section:'studio.run.pipe.t2m',label:'studio.music.ace.generateAudioCodes',target:'[data-ace-setting="generateAudioCodes"]',aliases:['ACE-Step','music','generateAudioCodes'],reason:'studio.run.pipe.noRecipe'},
  {view:studio,studioTab:'generate',tab:'setup.tab.studio',section:'studio.run.pipe.t2m',label:'studio.music.ace.lmCfg',target:'[data-ace-setting="lmCfg"]',aliases:['ACE-Step','music','lmCfg'],reason:'studio.run.pipe.noRecipe'},
  {view:studio,studioTab:'generate',tab:'setup.tab.studio',section:'studio.run.pipe.t2m',label:'studio.music.ace.temperature',target:'[data-ace-setting="temperature"]',aliases:['ACE-Step','music','temperature'],reason:'studio.run.pipe.noRecipe'},
  {view:studio,studioTab:'generate',tab:'setup.tab.studio',section:'studio.run.pipe.t2m',label:'studio.music.ace.topP',target:'[data-ace-setting="topP"]',aliases:['ACE-Step','music','topP'],reason:'studio.run.pipe.noRecipe'},
  {view:studio,studioTab:'generate',tab:'setup.tab.studio',section:'studio.run.pipe.t2m',label:'studio.music.ace.topK',target:'[data-ace-setting="topK"]',aliases:['ACE-Step','music','topK'],reason:'studio.run.pipe.noRecipe'},
  {view:studio,studioTab:'generate',tab:'setup.tab.studio',section:'studio.run.pipe.t2m',label:'studio.music.ace.minP',target:'[data-ace-setting="minP"]',aliases:['ACE-Step','music','minP'],reason:'studio.run.pipe.noRecipe'},
  {view:studio,studioTab:'generate',tab:'setup.tab.studio',section:'studio.run.pipe.t2m',label:'studio.music.maxDuration',target:'[data-music-duration]',aliases:['ACE-Step','music'],reason:'studio.run.pipe.noRecipe'},
  {view:studio,studioTab:'generate',tab:'setup.tab.studio',section:'studio.run.pipe.t2m',label:'studio.music.style',target:'[data-music-prompt="style"]',aliases:['ACE-Step','music'],reason:'studio.run.pipe.noRecipe'},
  {view:studio,studioTab:'generate',tab:'setup.tab.studio',section:'studio.run.pipe.t2m',label:'studio.music.lyrics',target:'[data-music-prompt="lyrics"]',aliases:['ACE-Step','music'],reason:'studio.run.pipe.noRecipe'},
  {view:studio,studioTab:'generate',tab:'setup.tab.studio',section:'studio.run.pipe.t2m',label:'studio.run.textEncoder',target:'[data-run-component="textEncoders"] select',aliases:['ACE-Step','music'],reason:'studio.run.pipe.noRecipe'},
  {view:studio,studioTab:'generate',tab:'setup.tab.studio',section:'studio.run.pipe.t2m',label:'studio.run.textEncoder2',target:'[data-run-component="textEncoders2"] select',aliases:['ACE-Step','music'],reason:'studio.run.pipe.noRecipe'},
  {view:studio,studioTab:'generate',tab:'setup.tab.studio',section:'studio.run.pipe.t2m',label:'studio.run.vae',target:'[data-run-component="vaes"] select',aliases:['ACE-Step','music'],reason:'studio.run.pipe.noRecipe'},
  // Studio · prompts and Run
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'search.section.prompts', label: 'studio.run.positive', target: '#run-positive', aliases: ['prompt', 'text'] },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'search.section.prompts', label: 'studio.run.negative', target: '#run-negative', aliases: ['prompt', 'exclude'] },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'search.section.run', label: 'studio.run.run', target: '[data-run]', aliases: ['generate', 'submit'], reason: 'search.reason.backend' },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'search.section.run', label: 'studio.run.stop.choose', target: '[data-run-stop]', aliases: ['stop current run', 'stop all', 'cancel', 'interrupt', 'queue'], reason: 'studio.run.queueEmpty' },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.queue', label: 'studio.run.queue', target: '#studio-queue' },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.metadata', label: 'studio.run.metadata.apply', target: '[data-media-settings]', aliases: ['reuse', 'restore', 'media settings', 'metadata'], reason: 'studio.run.metadata.empty' },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.queue', label: 'studio.run.metadata.reuse', target: '[data-job-reuse-prompt]', aliases: ['reuse prompt', 'queue prompt'], reason: 'studio.run.metadata.empty' },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.queue', label: 'studio.run.exportClean.label', target: '[data-job-export-clean]', aliases: ['export clean', 'strip metadata', 'remove metadata', 'privacy'], reason: 'studio.run.exportClean.needs' },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.edit.title', label: 'studio.run.exportClean.label', target: '[data-media-export-clean]', aliases: ['export clean', 'strip metadata', 'remove metadata', 'privacy'], reason: 'studio.run.exportClean.needs' },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.viewer.open', label: 'studio.run.viewer.prev', target: '[data-viewer-prev]', aliases: ['previous image', 'back', 'left arrow'], reason: 'studio.run.viewer.closed' },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.viewer.open', label: 'studio.run.viewer.next', target: '[data-viewer-next]', aliases: ['next image', 'forward', 'right arrow'], reason: 'studio.run.viewer.closed' },
  { view: studio, studioTab: 'generate', dockTab: 'media', tab: 'setup.tab.studio', section: 'studio.run.dock.media', label: 'media.intake.label', target: '#studio-dock [data-media-intake]', aliases: ['paste', 'clipboard', 'copy into input', 'screenshot', 'import'] },
  // SAM masking and refinement
  { view: studio, studioTab: 'generate', dockTab: 'post', tab: 'setup.tab.studio', section: 'studio.sam.title', label: 'studio.sam.model', target: '[data-sam="model"]', aliases: ["SAM", "checkpoint", "segment anything"], reason: 'studio.sam.refineHelp' },
  { view: studio, studioTab: 'generate', dockTab: 'post', tab: 'setup.tab.studio', section: 'studio.sam.title', label: 'studio.sam.targets', target: '[data-sam="targets"]', aliases: ["SAM", "eyes", "feet", "hands", "objects"], reason: 'studio.sam.refineHelp' },
  { view: studio, studioTab: 'generate', dockTab: 'post', tab: 'setup.tab.studio', section: 'studio.sam.title', label: 'studio.sam.threshold', target: '[data-sam="threshold"]', aliases: ["SAM", "confidence"], reason: 'studio.sam.refineHelp' },
  { view: studio, studioTab: 'generate', dockTab: 'post', tab: 'setup.tab.studio', section: 'studio.sam.title', label: 'studio.sam.denoise', target: '[data-sam="denoise"]', aliases: ["SAM", "strength"], reason: 'studio.sam.refineHelp' },
  { view: studio, studioTab: 'generate', dockTab: 'post', tab: 'setup.tab.studio', section: 'studio.sam.title', label: 'studio.run.maskGrow', target: '[data-sam="margin"]', aliases: ["SAM", "crop", "context", "margin"], reason: 'studio.sam.refineHelp' },
  { view: studio, studioTab: 'generate', dockTab: 'post', tab: 'setup.tab.studio', section: 'studio.sam.title', label: 'studio.sam.enabled', target: '[data-sam="enabled"]', aliases: ["SAM", "automatic", "post generation"], reason: 'studio.sam.refineHelp' },
  { view: studio, studioTab: 'generate', dockTab: 'post', tab: 'setup.tab.studio', section: 'studio.sam.title', label: 'studio.sam.refine', target: '[data-sam="refine"]', aliases: ["SAM", "existing image", "manual refine"], reason: 'studio.sam.refineHelp' },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.sam.title', label: 'studio.sam.mask', target: '[data-sam="mask"]', aliases: ['SAM', 'text mask', 'segment'], reason: 'studio.sam.inpaint' },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.sam.title', label: 'studio.editor.samPoint', target: '[data-sam-tool="samPoint"]', aliases: ['SAM', 'click point negative Alt'], reason: 'studio.sam.inpaint' },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.sam.title', label: 'studio.editor.samBox', target: '[data-sam-tool="samBox"]', aliases: ['SAM', 'box rectangle object'], reason: 'studio.sam.inpaint' },
  // Studio · bottom dock
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.dock', label: 'studio.run.dock.options', target: '[data-dock-tab="options"]', aliases: ['settings'] },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.dock', label: 'studio.run.dock.post', target: '[data-dock-tab="post"]', aliases: ['refine', 'upscale'] },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.dock', label: 'studio.run.dock.loras', target: '[data-dock-tab="loras"]', aliases: ['lora'] },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.dock', label: 'presets.title', target: '[data-dock-tab="presets"]', aliases: ['preset'] },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.dock', label: 'wildcards.title', target: '[data-dock-tab="wildcards"]', aliases: ['wildcard'] },
  { view: studio, studioTab: 'generate', tab: 'setup.tab.studio', section: 'studio.run.dock', label: 'studio.run.dock.references', target: '[data-open-references]', aliases: ['reference', 'image', 'refmod', 'create refmod'], reason: 'search.reason.references' },
  // Studio · Options dock cards
  { view: studio, studioTab: 'generate', dockTab: 'options', tab: 'setup.tab.studio', section: 'studio.run.dock.options', label: 'studio.run.opt.attention', target: '[data-option="attention"]', aliases: ['sage', 'flash', 'kitchen'], reason: 'search.reason.backend' },
  { view: studio, studioTab: 'generate', dockTab: 'options', tab: 'setup.tab.studio', section: 'studio.run.dock.options', label: 'studio.run.opt.enhancedPreview', target: '[data-option="enhancedPreview"]', aliases: ['animated', 'temporal', 'tiny vae', 'video preview'], reason: 'search.reason.backend' },
  { view: studio, studioTab: 'generate', dockTab: 'options', tab: 'setup.tab.studio', section: 'studio.run.dock.options', label: 'studio.run.opt.vae', target: '[data-option="vae"]', aliases: ['tiled', 'tile'], reason: 'search.reason.backend' },
  { view: studio, studioTab: 'generate', dockTab: 'options', tab: 'setup.tab.studio', section: 'studio.run.dock.options', label: 'studio.run.opt.cache', target: '[data-option="cache"]', aliases: ['teacache', 'magcache', 'easycache', 'threshold'], reason: 'search.reason.backend' },
  { view: studio, studioTab: 'generate', dockTab: 'options', tab: 'setup.tab.studio', section: 'studio.run.dock.options', label: 'studio.run.opt.compile', target: '[data-option="compile"]', aliases: ['torch'], reason: 'search.reason.backend' },
  { view: studio, studioTab: 'generate', dockTab: 'options', tab: 'setup.tab.studio', section: 'studio.run.dock.options', label: 'studio.run.opt.chunking', target: '[data-option="chunking"]', aliases: ['chunk', 'memory', 'video'], reason: 'search.reason.backend' },
  { view: studio, studioTab: 'generate', dockTab: 'options', tab: 'setup.tab.studio', section: 'studio.run.dock.options', label: 'studio.run.opt.sigmas', target: '[data-option="sigmas"]', aliases: ['schedule', 'noise'] },
  { view: studio, studioTab: 'generate', dockTab: 'options', tab: 'setup.tab.studio', section: 'studio.run.dock.options', label: 'studio.run.opt.output', target: '[data-option="output"]', aliases: ['format', 'png', 'webp', 'jpeg', 'quality', 'codec'] },
  { view: studio, studioTab: 'generate', dockTab: 'options', tab: 'setup.tab.studio', section: 'studio.run.dock.options', label: 'studio.run.opt.outputNaming', target: '[data-option="outputNaming"]', aliases: ['filename', 'prefix'] },
  // Prompt Forge
  { view: studio, studioTab: 'forge', tab: 'studio.tab.forge', section: 'studio.tab.forge', label: 'forge.see', target: '[data-forge-see], [data-forge-auto-see]', aliases: ['vision', 'pictures', 'reference images', 'keyframes'] },
  { view: studio, studioTab: 'forge', tab: 'studio.tab.forge', section: 'studio.tab.forge', label: 'forge.brief', target: '[data-forge-brief]', aliases: ['prompt', 'idea'] },
  { view: studio, studioTab: 'forge', tab: 'studio.tab.forge', section: 'studio.tab.forge', label: 'forge.result', target: '[data-forge-result]', aliases: ['prompt', 'output'] },
  { view: studio, studioTab: 'forge', tab: 'studio.tab.forge', section: 'studio.tab.forge', label: 'forge.negative', target: '[data-forge-negative]', aliases: ['prompt'] },
  { view: studio, studioTab: 'forge', tab: 'studio.tab.forge', section: 'studio.tab.forge', label: 'forge.creativity', target: '[data-forge-ladder="creativity"]', aliases: ['temperature', 'level'] },
  { view: studio, studioTab: 'forge', tab: 'studio.tab.forge', section: 'studio.tab.forge', label: 'forge.detail', target: '[data-forge-ladder="detail"]', aliases: ['length', 'level'] },
  // Chat
  { view: studio, studioTab: 'chat', tab: 'studio.tab.chat', section: 'studio.tab.chat', label: 'chat.message', target: '[data-chat-input]', aliases: ['prompt', 'ask'] },
  { view: studio, studioTab: 'generate', dockTab: 'post', tab: 'setup.tab.studio', section: 'studio.audioEnhance.title', label: 'studio.audioEnhance.deep', target: '[data-audio-enhance="post"] [data-audio-enhance-install="deep"]', aliases: ['audio', 'install', 'download', 'retry', 'engine', 'DeepFilterNet', 'speech', 'license', 'consent'], reason: 'studio.audioEnhance.modelsMissing' },
  { view: studio, studioTab: 'generate', dockTab: 'post', tab: 'setup.tab.studio', section: 'studio.audioEnhance.title', label: 'studio.audioEnhance.stems', target: '[data-audio-enhance="post"] [data-audio-enhance-install="stems"]', aliases: ['audio', 'install', 'download', 'retry', 'engine', 'Smule', 'MIT', 'Windowed RoFormer', 'vocals', 'instrumental', 'license', 'consent', 'checkpoint'], reason: 'studio.audioEnhance.modelsMissing' },
  { view: studio, studioTab: 'tve', tab: 'studio.tab.tve', section: 'studio.audioEnhance.title', label: 'studio.audioEnhance.deep', target: '[data-audio-enhance="tve"] [data-audio-enhance-install="deep"]', aliases: ['audio', 'install', 'download', 'retry', 'engine', 'DeepFilterNet', 'speech', 'license', 'consent'], reason: 'studio.audioEnhance.modelsMissing' },
  { view: studio, studioTab: 'tve', tab: 'studio.tab.tve', section: 'studio.audioEnhance.title', label: 'studio.audioEnhance.stems', target: '[data-audio-enhance="tve"] [data-audio-enhance-install="stems"]', aliases: ['audio', 'install', 'download', 'retry', 'engine', 'Smule', 'MIT', 'Windowed RoFormer', 'vocals', 'instrumental', 'license', 'consent', 'checkpoint'], reason: 'studio.audioEnhance.modelsMissing' },
  { view: studio, studioTab: 'generate', dockTab: 'post', tab: 'setup.tab.studio', section: 'studio.run.dock.post', label: 'studio.audioEnhance.title', target: '[data-audio-enhance="post"] [data-audio-enhance-mode]', aliases: ['audio', 'sound', 'denoise', 'loudness', 'stems'], reason: 'studio.audioEnhance.needsSource' },
  { view: studio, studioTab: 'tve', tab: 'studio.tab.tve', section: 'studio.tve.post.title', label: 'studio.audioEnhance.title', target: '[data-audio-enhance="tve"] [data-audio-enhance-mode]', aliases: ['audio', 'sound', 'denoise', 'loudness', 'stems'], reason: 'studio.audioEnhance.needsSource' },
  { view: studio, studioTab: 'generate', dockTab: 'post', tab: 'setup.tab.studio', section: 'studio.audioEnhance.title', label: 'studio.audioEnhance.profile', target: '[data-audio-enhance="post"] [data-audio-enhance-profile]', aliases: ['audio', 'sound'], reason: 'studio.audioEnhance.hint' },
  { view: studio, studioTab: 'generate', dockTab: 'post', tab: 'setup.tab.studio', section: 'studio.audioEnhance.title', label: 'studio.audioEnhance.save', target: '[data-audio-enhance="post"] [data-audio-enhance-run]', aliases: ['audio', 'sound'], reason: 'studio.audioEnhance.hint' },
  { view: studio, studioTab: 'generate', dockTab: 'post', tab: 'setup.tab.studio', section: 'studio.audioEnhance.title', label: 'studio.audioEnhance.cancel', target: '[data-audio-enhance="post"] [data-audio-enhance-cancel]', aliases: ['audio', 'sound'], reason: 'studio.audioEnhance.hint' },
  { view: studio, studioTab: 'generate', dockTab: 'post', tab: 'setup.tab.studio', section: 'studio.audioEnhance.title', label: 'studio.audioEnhance.installCancel', target: '[data-audio-enhance="post"] [data-audio-enhance-install-cancel]', aliases: ['audio', 'sound'], reason: 'studio.audioEnhance.hint' },
  { view: studio, studioTab: 'tve', tab: 'studio.tab.tve', section: 'studio.audioEnhance.title', label: 'studio.audioEnhance.profile', target: '[data-audio-enhance="tve"] [data-audio-enhance-profile]', aliases: ['audio', 'sound'], reason: 'studio.audioEnhance.hint' },
  { view: studio, studioTab: 'tve', tab: 'studio.tab.tve', section: 'studio.audioEnhance.title', label: 'studio.audioEnhance.save', target: '[data-audio-enhance="tve"] [data-audio-enhance-run]', aliases: ['audio', 'sound'], reason: 'studio.audioEnhance.hint' },
  { view: studio, studioTab: 'tve', tab: 'studio.tab.tve', section: 'studio.audioEnhance.title', label: 'studio.audioEnhance.cancel', target: '[data-audio-enhance="tve"] [data-audio-enhance-cancel]', aliases: ['audio', 'sound'], reason: 'studio.audioEnhance.hint' },
  { view: studio, studioTab: 'tve', tab: 'studio.tab.tve', section: 'studio.audioEnhance.title', label: 'studio.audioEnhance.installCancel', target: '[data-audio-enhance="tve"] [data-audio-enhance-install-cancel]', aliases: ['audio', 'sound'], reason: 'studio.audioEnhance.hint' },
  { view: studio, studioTab: 'generate', dockTab: 'post', tab: 'setup.tab.studio', section: 'studio.audioEnhance.title', label: 'studio.audioEnhance.engines', target: '[data-audio-enhance="post"] [data-audio-enhance-engines]', aliases: ['audio', 'engine', 'runtime', 'install'], reason: 'studio.audioEnhance.modelsMissing' },
  { view: studio, studioTab: 'tve', tab: 'studio.tab.tve', section: 'studio.audioEnhance.title', label: 'studio.audioEnhance.engines', target: '[data-audio-enhance="tve"] [data-audio-enhance-engines]', aliases: ['audio', 'engine', 'runtime', 'install'], reason: 'studio.audioEnhance.modelsMissing' },
  { view: studio, studioTab: 'generate', dockTab: 'post', tab: 'setup.tab.studio', section: 'studio.audioEnhance.title', label: 'studio.audioEnhance.device', target: '[data-audio-enhance="post"] [data-audio-enhance-device]', aliases: ['audio', 'device', 'GPU', 'CUDA', 'CPU', 'automatic'], reason: 'studio.audioEnhance.deviceHint' },
  { view: studio, studioTab: 'tve', tab: 'studio.tab.tve', section: 'studio.audioEnhance.title', label: 'studio.audioEnhance.device', target: '[data-audio-enhance="tve"] [data-audio-enhance-device]', aliases: ['audio', 'device', 'GPU', 'CUDA', 'CPU', 'automatic'], reason: 'studio.audioEnhance.deviceHint' },
  // TVE
  { view: studio, studioTab: 'tve', tab: 'studio.tab.tve', section: 'studio.tve.post.title', label: 'studio.tve.post.model', target: '[data-tve-post="upscaleModel"]', aliases: ['upscale'] },
  { view: studio, studioTab: 'tve', tab: 'studio.tab.tve', section: 'studio.tve.post.title', label: 'studio.tve.post.frameGen', target: '[data-tve-post="frameGenEnabled"]', aliases: ['fps', 'interpolate'] },
  { view: studio, studioTab: 'tve', tab: 'studio.tab.tve', section: 'studio.tve.post.title', label: 'studio.tve.post.loop', target: '[data-tve-backend] [data-tve-post="loopEnabled"]', aliases: ['loop', 'seamless', 'repeat', 'crossfade', 'beat'], reason: 'studio.tve.post.loopMissing' },
  { view: studio, studioTab: 'generate', dockTab: 'post', tab: 'setup.tab.studio', section: 'studio.tve.post.title', label: 'studio.tve.post.loop', target: '.wb-post [data-tve-post="loopEnabled"]', aliases: ['loop', 'seamless', 'repeat', 'crossfade', 'beat'], reason: 'studio.tve.post.loopMissing' },
  { view: studio, studioTab: 'tve', tab: 'studio.tab.tve', section: 'studio.tve.post.title', label: 'studio.run.upscale', target: '[data-tve-post="rtxScale"]' },
  { view: studio, studioTab: 'tve', tab: 'studio.tab.tve', section: 'studio.tve.post.title', label: 'studio.tve.post.chunkFrames', target: '[data-tve-post="chunkFrames"]', aliases: ['memory'] },
  { view: studio, studioTab: 'tve', tab: 'studio.tab.tve', section: 'studio.tve.post.title', label: 'studio.tve.post.multiplier', target: '[data-tve-post="multiplier"]', aliases: ['frame rate', 'fps'] },
  { view: studio, studioTab: 'monitor', tab: 'monitoring.tab', section: 'monitoring.generations', label: 'monitoring.col.duration', target: '[data-monitoring-headline]', aliases: ['duration', 'time', 'history'] },
  { view: studio, studioTab: 'monitor', tab: 'monitoring.tab', section: 'monitoring.system', label: 'monitoring.cpu', target: '[data-monitoring-metric="cpu"]', aliases: ['cpu', 'ram', 'gpu', 'vram', 'telemetry'] },
  // Run Plan stages (a stage that the draft does not use is genuinely unavailable)
  { view: studio, studioTab: 'map', tab: 'studio.tab.map', section: 'studio.tab.map', label: 'studio.plan.model', target: '[data-plan-stage="model"]', aliases: ['checkpoint'], reason: 'search.reason.plan' },
  { view: studio, studioTab: 'map', tab: 'studio.tab.map', section: 'studio.tab.map', label: 'studio.plan.prompt', target: '[data-plan-stage="prompt"]', aliases: ['prompt'], reason: 'search.reason.plan' },
  { view: studio, studioTab: 'map', tab: 'studio.tab.map', section: 'studio.tab.map', label: 'studio.plan.generate', target: '[data-plan-stage="generate"]', aliases: ['steps', 'sampler'], reason: 'search.reason.plan' },
  { view: studio, studioTab: 'map', tab: 'studio.tab.map', section: 'studio.tab.map', label: 'studio.plan.output', target: '[data-plan-stage="output"]', aliases: ['save', 'folder'], reason: 'search.reason.plan' },
  // Library
  { view: 'downloads', libraryView: 'catalog', tab: 'downloads.title', section: 'search.section.filters', label: 'downloads.search', target: '[data-download-search]', aliases: ['find', 'model'] },
  { view: 'downloads', libraryView: 'catalog', tab: 'downloads.title', section: 'search.section.filters', label: 'downloads.catalogSource', target: '[data-download-source]', aliases: ['civitai'] },
  { view: 'downloads', libraryView: 'catalog', tab: 'downloads.title', section: 'downloads.view.catalog', label: 'downloads.view.catalog', target: '[data-library-tab="catalog"]' },
  { view: 'downloads', libraryView: 'installed', tab: 'downloads.title', section: 'downloads.view.installed', label: 'downloads.view.installed', target: '[data-library-tab="installed"]', aliases: ['local'], reason: 'search.reason.installed' },
  { view: 'downloads', libraryView: 'installed', tab: 'downloads.title', section: 'downloads.view.installed', label: 'downloads.installed.scanAll', target: '[data-scan-files]', aliases: ['metadata', 'civitai'], reason: 'search.reason.installed' },
  { view: 'downloads', libraryView: 'installed', tab: 'downloads.title', section: 'downloads.view.installed', label: 'downloads.installed.scanScope', target: '[data-scan-scope]', aliases: ['scan', 'scope'], reason: 'search.reason.installed' },
  // Comfy
  { view: 'comfy', tab: 'setup.tab.comfy', section: 'setup.comfy.title', label: 'setup.comfy.title', target: 'iframe', aliases: ['workflow', 'editor'], reason: 'search.reason.backend' },
  // Backend
  { view: 'backend', panel: 'startup', tab: 'settings.button', section: 'startup.title', label: 'startup.preset', target: '.startup-presets button', aliases: ['flags', 'optimization'], reason: 'search.reason.installed' },
  { view: 'backend', panel: 'llm', tab: 'settings.button', section: 'search.section.panes', label: 'llm.title', target: '.llm-panel', aliases: ['endpoint', 'server'], reason: 'search.reason.installed' },
  { view: 'backend', panel: 'auth', tab: 'settings.button', section: 'search.section.panes', label: 'downloads.auth', target: '.auth-providers button', aliases: ['token', 'civitai'] },
  { view: 'backend', tab: 'settings.button', section: 'setup.backendLifecycle.title', label: 'setup.backendAutostart', target: '.backend-autostart', aliases: ['start', 'autostart'] },
  { view: 'backend', tab: 'settings.button', section: 'setup.backendLifecycle.title', label: 'setup.backendLifecycle.start', target: '[data-backend-action="start"]', aliases: ['start', 'comfyui', 'run backend'], reason: 'search.reason.backendRunning' },
  { view: 'backend', tab: 'settings.button', section: 'setup.backendLifecycle.title', label: 'setup.backendLifecycle.stop', target: '[data-backend-action="stop"]', aliases: ['stop', 'kill', 'shutdown'], reason: 'search.reason.backend' },
  { view: 'backend', tab: 'settings.button', section: 'setup.backendLifecycle.title', label: 'setup.backendLifecycle.restart', target: '[data-backend-action="restart"]', aliases: ['restart', 'reload'], reason: 'search.reason.backend' },
  { view: 'backend', tab: 'settings.button', section: 'setup.backendLifecycle.title', label: 'setup.backendLifecycle.test', target: '[data-backend-action="test"]', aliases: ['test', 'check', 'dependencies'] },
  { view: 'backend', tab: 'settings.button', section: 'setup.backendLifecycle.title', label: 'setup.backendLifecycle.inspect', target: '[data-backend-action="inspect"]', aliases: ['inspect', 'connect', 'endpoint'], reason: 'search.reason.backend' },
  { view: 'backend', tab: 'settings.button', section: 'setup.backendLifecycle.title', label: 'setup.pickParent', target: '[data-setup-action="pick"]', aliases: ['environment', 'folder', 'install location'] },
  { view: 'backend', tab: 'settings.button', section: 'setup.backendLifecycle.title', label: 'setup.reinstall', target: '[data-setup-action="reinstall"]', aliases: ['reinstall', 'repair', 'venv'], reason: 'search.reason.installed' },
  { view: 'backend', tab: 'settings.button', section: 'settings.title', label: 'settings.prompt.spellcheck', target: '[data-prompt-tool="spellcheck"]', aliases: ['spelling', 'dictionary'] },
  { view: 'backend', tab: 'settings.button', section: 'settings.title', label: 'settings.prompt.autocomplete', target: '[data-prompt-tool="autocomplete"]', aliases: ['tags', 'suggest', 'danbooru'] },
  { view: 'backend', tab: 'settings.button', section: 'settings.title', label: 'settings.models', target: '[data-folder="models"]', aliases: ['folder', 'directory'], reason: 'search.reason.installed' },
  { view: 'backend', tab: 'settings.button', section: 'settings.title', label: 'settings.input', target: '[data-folder="input"]', aliases: ['folder', 'directory'], reason: 'search.reason.installed' },
  { view: 'backend', tab: 'settings.button', section: 'settings.title', label: 'settings.output', target: '[data-folder="output"]', aliases: ['folder', 'directory'], reason: 'search.reason.installed' },
  { view: 'backend', tab: 'settings.button', section: 'settings.title', label: 'app.language', target: '#language-select', aliases: ['locale', 'translation'] },
  { view: 'backend', tab: 'settings.button', section: 'settings.button', label: 'setup.upgrade', target: '[data-upgrade]', aliases: ['update'] },
  // Logs
  { view: 'logs', tab: 'logs.title', section: 'search.section.filters', label: 'logs.search', target: '.logs-search', aliases: ['find', 'filter'] },
  { view: 'logs', tab: 'logs.title', section: 'search.section.filters', label: 'logs.sources', target: '[data-logs-group="source"] button', aliases: ['filter'] },
  { view: 'logs', tab: 'logs.title', section: 'search.section.filters', label: 'logs.levels', target: '[data-logs-group="level"] button', aliases: ['warn', 'error'] },
  { view: 'logs', tab: 'logs.title', section: 'logs.lines', label: 'logs.lines', target: '.logs-view', aliases: ['output'] },
  { view: 'logs', tab: 'logs.title', section: 'search.section.filters', label: 'logs.copy', target: '[data-logs-action="copy"]', aliases: ['clipboard'], reason: 'search.reason.lines' },
  { view: 'logs', tab: 'logs.title', section: 'search.section.filters', label: 'logs.export', target: '[data-logs-action="export"]', aliases: ['save', 'file'], reason: 'search.reason.lines' },
];

/** Case-insensitive substring match over the localized breadcrumb plus the English aliases. */
export function matchEntries(
  entries: readonly SearchEntry[],
  query: string,
  text: (key: string) => string,
  limit = 12,
): SearchEntry[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return entries.slice(0, limit);
  return entries
    .filter((entry) =>
      [text(entry.label), text(entry.section), text(entry.tab), ...(entry.aliases ?? [])].some((value) =>
        value.toLowerCase().includes(needle),
      ),
    )
    .slice(0, limit);
}

export type RevealResult = 'revealed' | 'disabled' | 'missing';

/** `title` is the disabled control's own explanation, when it has one. */
export interface RevealOutcome {
  state: RevealResult;
  title?: string;
}

/** Opens collapsed ancestors, scrolls to the control and flashes it; reports a currently disabled control. */
export function revealTarget(target: string): RevealOutcome {
  if(target==='.experience-switch'||target.startsWith('[data-theme-action='))document.querySelector<HTMLButtonElement>('.studio-quick > button[aria-expanded="false"]')?.click();
  const element = document.querySelector<HTMLElement>(target);
  if (!element) return { state: 'missing' };
  for (let parent = element.parentElement; parent; parent = parent.parentElement) {
    if (parent instanceof HTMLDetailsElement) parent.open = true;
  }
  element.scrollIntoView?.({ block: 'center' });
  element.classList.add('search-flash');
  window.setTimeout(() => element.classList.remove('search-flash'), 1400);
  const disabled = element.matches(':disabled') || !!element.querySelector(':disabled:not(option):not(optgroup)') || element.getAttribute('aria-disabled') === 'true';
  if (!disabled) return { state: 'revealed' };
  const title = element.getAttribute('title')?.trim();
  return { state: 'disabled', ...(title ? { title: title.slice(0, 160) } : {}) };
}
