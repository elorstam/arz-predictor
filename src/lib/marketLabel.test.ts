import {describe,it,expect} from 'vitest';
import {renderToStaticMarkup} from 'react-dom/server';
import {createElement} from 'react';
import {marketLabel} from './format';
describe('persisted corner selection rendering',()=>{
  it('preserves exact line and outcome for both normalized corner names',()=>{
    for(const market of ['FULL_TIME_TOTAL_CORNERS','CORNERS_TOTAL']) {
      for(const [selection,line,label] of [['OVER',8.5,'Toplam Korner 8.5 Üst'],['OVER',9.5,'Toplam Korner 9.5 Üst'],['UNDER',10.5,'Toplam Korner 10.5 Alt']] as const) {
        expect(renderToStaticMarkup(createElement('span',null,marketLabel(market,selection,line)))).toContain(label);
      }
    }
  });
  it('does not invent a missing line or outcome',()=>{
    expect(marketLabel('FULL_TIME_TOTAL_CORNERS',undefined,null)).toContain('eksik');
    expect(marketLabel('FULL_TIME_TOTAL_CORNERS',undefined,null)).not.toContain('Üst');
  });
});
