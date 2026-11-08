import { expect, test } from 'vitest';
import { activePost, activeWildcards, changedOptions } from './dockBadges';
import { defaultOptions } from './OptionsPanel';
import { defaultTve } from './TvePostControls';
import { defaultWildcard } from '../../lib/wildcard';

test('options count only fields that differ from defaults; absent older fields are default',()=>{
 expect(changedOptions(defaultOptions)).toEqual([]);
 expect(changedOptions({attention:'',vae:'full',tile:512,cache:'',threshold:0.05,compile:''})).toEqual([]);
 expect(changedOptions({...defaultOptions,vae:'tiled',cache:'EasyCache',output:{...defaultOptions.output!}})).toEqual(['vae','cache']);
});
test('wildcards count the switch and authored rules, never automatic seed/reroll/picks',()=>{
 expect(activeWildcards(false,{...defaultWildcard,style:'Natural Language'})).toEqual([]);
 expect(activeWildcards(true,{...defaultWildcard,seed:9,reroll:3,selection_state:{a:{enabled:true,weight:1}}})).toEqual(['enabled']);
 expect(activeWildcards(true,{...defaultWildcard,style:'Natural Language',token_budget:300})).toEqual(['enabled','style','token_budget']);
});
test('post counts the stages that run for the current pipeline',()=>{
 const base={video:false,audio:false,latent:false,tve:defaultTve,refine:false,sam:false,audioMode:'off'};
 expect(activePost(base)).toEqual([]);
 expect(activePost({...base,refine:true,sam:true,latent:true,tve:{...defaultTve,frameGen:true}})).toEqual(['refine','sam']);
 expect(activePost({...base,video:true,audio:true,audioMode:'clean',latent:true,tve:{...defaultTve,upscaleModel:'x.pth',denoise:true,frameGen:true},refine:true})).toEqual(['latent','upscale','denoise','frameGen','audio']);
 expect(activePost({...base,video:true,audio:false,audioMode:'clean'})).toEqual([]);
});
