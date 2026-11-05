import { Graphics } from 'pixi.js';
import type { AgentKind } from '../ipc/generated/AgentKind';

// Original 16px art. Every pixel uses the same sixteen-colour application palette.
export const ink = {
  ground: '#132f35',
  deep: '#1d4145',
  panel: '#2c4b4d',
  light: '#52716b',
  line: '#10292c',
  text: '#f1e7c7',
  muted: '#c0cfba',
  grass: '#478744',
  grassDark: '#255a40',
  path: '#c9a26b',
  wood: '#805832',
  claude: '#e89c48',
  codex: '#49bdd1',
  ok: '#9cc75b',
  warn: '#81e7e8',
  danger: '#e56757',
} as const;
type PixelColour = keyof typeof ink;
const maps = {
  arrival: [
    '................',
    '..llllllllllll..',
    '.lmmmmmmmmmmmml.',
    '.lmddddddddddml.',
    '.lmdmmmmmmmmdml.',
    '.lmdmddddddmdml.',
    '.lmdmdmmmmdmdml.',
    '.lmdmdmddmdmdml.',
    '.lmdmdmmmmdmdml.',
    '.lmdmddddddmdml.',
    '.lmdmmmmmmmmdml.',
    '.lmddddddddddml.',
    '.lmmmmmmmmmmmml.',
    '..llllllllllll..',
    '................',
    '................',
  ],
  desk: [
    '...llllllllll...',
    '...lccccccccl...',
    '...lcddddddcl...',
    '...lcddddddcl...',
    '...lccccccccl...',
    '...llllmlllll...',
    '......lml.......',
    '.llllllllllllll.',
    '.lppppppppppppl.',
    '.lwwwwwwwwwwwwl.',
    '.llllllllllllll.',
    '..ll........ll..',
    '..ll........ll..',
    '..ll........ll..',
    '................',
    '................',
  ],
  workbench: [
    '...........ll...',
    '..ll......lwwl..',
    '..lppl....lwwl..',
    '...lppl...lwwl..',
    '....lppl..llll..',
    '.....lppl..ll...',
    '......lppl.ll...',
    '.llllllllllllll.',
    '.lppppppppppppl.',
    '.lwwwwwwwwwwwwl.',
    '.llllllllllllll.',
    '..ll........ll..',
    '..ll........ll..',
    '..ll........ll..',
    '................',
    '................',
  ],
  test: [
    '....llllllll....',
    '....lmmmmmml....',
    '.....lmmmml.....',
    '.....lmdml......',
    '.....lmdml......',
    '....lmddmml.....',
    '...lmddddmml....',
    '..lmoooooomml...',
    '..lmoooooomml...',
    '..lmoooooomml...',
    '...lllllllll....',
    '.llllllllllllll.',
    '.lwwwwwwwwwwwwl.',
    '..ll........ll..',
    '................',
    '................',
  ],
  gate: [
    '.lll........lll.',
    '.lppl......lppl.',
    '.lppl.llll.lppl.',
    '.lppl.lmml.lppl.',
    '.lppl.lmml.lppl.',
    '.lppl..ll..lppl.',
    '.lppl......lppl.',
    '.lwwllllllllwwl.',
    '.lwwppppppppwwl.',
    '.lwwllllllllwwl.',
    '.lppl......lppl.',
    '.lppl......lppl.',
    '.lppl......lppl.',
    '.llll......llll.',
    '................',
    '................',
  ],
  flag: [
    '...ll...........',
    '...lllllll......',
    '...lkooool......',
    '...lkoooool.....',
    '...lkoooool.....',
    '...lkoooool.....',
    '...lllllll......',
    '...ll...........',
    '...ll...........',
    '...ll...........',
    '...ll...........',
    '...ll...........',
    '..llll..........',
    '.lppppl.........',
    '..llll..........',
    '................',
  ],
  bench: [
    '................',
    '................',
    '.llllllllllllll.',
    '.lppppppppppppl.',
    '.lwwwwwwwwwwwwl.',
    '.llllllllllllll.',
    '..ll........ll..',
    '.llllllllllllll.',
    '.lppppppppppppl.',
    '.lwwwwwwwwwwwwl.',
    '.llllllllllllll.',
    '..ll........ll..',
    '..ll........ll..',
    '................',
    '................',
    '................',
  ],
  alarm: [
    '......llll......',
    '.....lrrrrl.....',
    '....lrrrrrrl....',
    '...lrrrkkrrrl...',
    '..lrrrrkkrrrrl..',
    '..lrrrrkkrrrrl..',
    '..lrrrrkkrrrrl..',
    '..lrrrrrrrrrrl..',
    '...lrrrkkrrrl...',
    '....lrrrrrrl....',
    '.....llllll.....',
    '......lwwl......',
    '......lwwl......',
    '....llllllll....',
    '................',
    '................',
  ],
  review: [
    '...llllllll.....',
    '...lkkkkkkl.....',
    '...lklklkkl.....',
    '...lkkkkkkl.....',
    '...lklklkkl.....',
    '...lkkkkkkl.....',
    '...lklklkkl.....',
    '...lkkkkkkl.....',
    '...llllllll.....',
    '........llll....',
    '.......lcccl....',
    '.......lccl.....',
    '........ll......',
    '................',
    '................',
    '................',
  ],
  repair: [
    '..ll......ll....',
    '..lppl...lppl...',
    '...lppl.lppl....',
    '....lpplppl.....',
    '.....lpppl......',
    '.....lpppl......',
    '....lpplppl.....',
    '...lppl.lppl....',
    '..lppl...lppl...',
    '..lll.....lll...',
    '................',
    '................',
    '................',
    '................',
    '................',
    '................',
  ],
  waiting: [
    '...llllllllll...',
    '...lppppppppl...',
    '....lmkkkkml....',
    '.....lmkkml.....',
    '......lmml......',
    '......lmml......',
    '.....lmkkml.....',
    '....lmkkkkml....',
    '...lppppppppl...',
    '...llllllllll...',
    '................',
    '................',
    '................',
    '................',
    '................',
    '................',
  ],
  tree: [
    '......llll......',
    '....llggggll....',
    '...lggggggggl...',
    '..lggggggggggl..',
    '..lggggggggggl..',
    '.lggggggggggggl.',
    '.lggggggggggggl.',
    '..lggggggggggl..',
    '...llllllllll...',
    '......lwwl......',
    '......lwwl......',
    '......lwwl......',
    '.....llwwll.....',
    '................',
    '................',
    '................',
  ],
  codexAgent: [
    '......llll......',
    '......lccl......',
    '...llllllllll...',
    '..lccccccccccl..',
    '..lcddddddddcl..',
    '..lcdkkddkkdcl..',
    '..lcddddddddcl..',
    '..lccccccccccl..',
    '...llllllllll...',
    '..llccccccccll..',
    '..lclcckkcclcl..',
    '..lclcckkcclcl..',
    '..lllcccccclll..',
    '....llllllll....',
    '....lddllddl....',
    '....llllllll....',
  ],
  agent: [
    '.....llllll.....',
    '....laaaaaal....',
    '...laaaaaaaal...',
    '...laaaaaaaal...',
    '....lttttttl....',
    '....ltlttltl....',
    '....lttttttl....',
    '.....lttttl.....',
    '....laaaaaal....',
    '...laaaaaaaal...',
    '..ltlaaaaaaltl..',
    '..lllaaaaallll..',
    '....laaaaaal....',
    '....llllllll....',
    '....lddllddl....',
    '....llllllll....',
  ],
} as const;
export type Prop = Exclude<keyof typeof maps, 'agent' | 'codexAgent'>;
const colours: Record<string, PixelColour> = {
  l: 'line',
  m: 'muted',
  d: 'deep',
  p: 'path',
  w: 'wood',
  c: 'codex',
  o: 'ok',
  k: 'text',
  r: 'danger',
  g: 'grassDark',
  t: 'text',
};
export function pixelSprite(
  prop: Prop | 'agent',
  x: number,
  y: number,
  kind?: AgentKind,
  pose: 0 | 1 | 2 | 3 = 0,
): Graphics {
  const drawing = new Graphics();
  maps[prop === 'agent' && kind === 'Codex' ? 'codexAgent' : prop].forEach(
    (row, py) => {
      for (let px = 0; px < row.length; px += 1) {
        const pixel =
          prop === 'agent' && pose === 2 && py === 5 && (px === 6 || px === 9)
            ? 't'
            : row.charAt(px);
        const colour =
          pixel === 'a'
            ? kind === 'Codex'
              ? 'codex'
              : 'claude'
            : colours[pixel];
        if (colour)
          drawing
            .rect(
              x +
                px +
                (prop === 'agent' && pose === 1 && py >= 9 && py <= 11 && px < 4
                  ? 1
                  : 0),
              y +
                py -
                (prop === 'agent' && pose === 1 && py >= 14 && px < 8 ? 1 : 0) -
                (prop === 'agent' && pose === 3 && py >= 9 && py <= 11 && px < 4
                  ? 2
                  : 0),
              1,
              1,
            )
            .fill(ink[colour]);
      }
    },
  );
  return drawing;
}

/** Original people-sized 8×10 sprites; every painted cell is one native map pixel. */
export function personSprite(kind: AgentKind, pose: 0 | 1 | 2 | 3): Graphics {
  const drawing = new Graphics();
  const paint = (
    x: number,
    y: number,
    w: number,
    h: number,
    colour: string,
  ) => {
    drawing.rect(x, y, w, h).fill(colour);
  };
  const codex = kind === 'Codex';
  const coat = codex ? ink.codex : kind === 'ClaudeCode' ? ink.claude : ink.ok;
  paint(2, 0, 4, 4, ink.line);
  if (codex) {
    paint(2, 1, 4, 2, coat);
    paint(3, 1, 1, 1, ink.text);
    paint(5, 1, 1, 1, ink.text);
    paint(6, 1, 1, 2, ink.line);
  } else {
    paint(2, 0, 4, 1, ink.wood);
    paint(2, 1, 4, 2, ink.path);
    paint(5, 1, 1, 1, ink.line);
  }
  paint(2, 4, 4, 4, coat);
  paint(3, 4, 1, 3, ink.text);
  paint(1, pose === 3 ? 3 : 5, 1, 2, coat);
  paint(6, pose === 3 ? 2 : 5, 1, 2, coat);
  paint(2, 8, 2, pose === 1 ? 1 : 2, ink.deep);
  paint(5, 8, 1, pose === 1 ? 2 : 1, ink.deep);
  if (pose === 2) paint(4, 3, 1, 1, ink.text);
  if (pose === 3) paint(7, 1, 1, 2, ink.warn);
  return drawing;
}
