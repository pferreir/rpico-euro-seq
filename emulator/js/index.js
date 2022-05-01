import React from 'react';
import ReactDOM from 'react-dom/client';

import { library, dom } from "@fortawesome/fontawesome-svg-core";
import { faAngleLeft, faAngleRight, faCircleDot } from "@fortawesome/free-solid-svg-icons";

import ActionBar from './ActionBar';

library.add(faAngleLeft);
library.add(faAngleRight);
library.add(faCircleDot);
dom.watch();

import('../pkg/index.js').catch(console.error).then(({ ui_encoder_left, ui_encoder_right, ui_encoder_switch, midi_new_message }) => {
  const root = ReactDOM.createRoot(document.getElementById('action-bar'));
  root.render(
    <React.StrictMode>
      <ActionBar onEncoderLeft={ui_encoder_left} onEncoderRight={ui_encoder_right} onEncoderPress={ui_encoder_switch} onKeyPress={(up, key) => midi_new_message([up, 1, key, 100])} />
    </React.StrictMode>
  );
});
