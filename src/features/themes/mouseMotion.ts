export type Point={x:number;y:number};
export const CHASE_MS=10_000;
export function moveMouse(from:Point,to:Point,seconds:number,speed=175):Point {
 const dx=to.x-from.x,dy=to.y-from.y,d=Math.hypot(dx,dy);
 const step=Math.min(d,Math.max(0,seconds)*speed);
 return d?{x:from.x+dx/d*step,y:from.y+dy/d*step}:from;
}
export function rapidKnocks(times:number[],now:number):number[] {
 return [...times.filter(t=>now-t<=1200),now];
}
