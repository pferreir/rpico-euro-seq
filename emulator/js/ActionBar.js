import React from 'react';
import PropTypes from 'prop-types';
import { Piano, KeyboardShortcuts, MidiNumbers } from 'react-piano';

export default function ActionBar({ onEncoderLeft, onEncoderRight, onEncoderPress, onKeyPress }) {
  const firstNote = MidiNumbers.fromNote('c4');
  const lastNote = MidiNumbers.fromNote('f6');
  const keyboardShortcuts = KeyboardShortcuts.create({
    firstNote: firstNote,
    lastNote: lastNote,
    keyboardConfig: KeyboardShortcuts.HOME_ROW,
  });

  return (
    <>
      <div>
        <span onClick={onEncoderLeft}><i className="button fa-solid fa-angle-left" id="button-left"></i></span>
        <span onMouseDown={() => onEncoderPress(true)} onMouseUp={() => onEncoderPress(false)}><i className="button fa-solid fa-circle-dot" id="button-center"></i></span>
        <span onClick={onEncoderRight}><i className="button fa-solid fa-angle-right" id="button-right"></i></span>
      </div>
      <Piano
        noteRange={{ first: firstNote, last: lastNote }}
        playNote={midiNumber => {
          console.log(midiNumber);
          onKeyPress(true, midiNumber);
        }}
        stopNote={midiNumber => {
          onKeyPress(false, midiNumber);
        }}
        width={1000}
        keyboardShortcuts={keyboardShortcuts}
      />
    </>
  )
}

ActionBar.propTypes = {
  onEncoderLeft: PropTypes.func,
  onEncoderRight: PropTypes.func,
  onEncoderPress: PropTypes.func,
  onKeyPress: PropTypes.func
}
