import { AbsoluteFill, Sequence, useVideoConfig } from 'remotion';
import { Intro } from './scenes/Intro';
import { SettingsUI } from './scenes/SettingsUI';
import { Overlay } from './scenes/Overlay';
import { Experience } from './scenes/Experience';
import { Outro } from './scenes/Outro';
import { colors } from './theme';

export const MainVideo: React.FC = () => {
  const { fps } = useVideoConfig();

  return (
    <AbsoluteFill style={{ backgroundColor: colors.bg }}>
      <Sequence from={0} durationInFrames={3 * fps}>
        <Intro />
      </Sequence>
      <Sequence from={3 * fps} durationInFrames={5 * fps}>
        <SettingsUI />
      </Sequence>
      <Sequence from={8 * fps} durationInFrames={4 * fps}>
        <Overlay />
      </Sequence>
      <Sequence from={12 * fps} durationInFrames={6 * fps}>
        <Experience />
      </Sequence>
      <Sequence from={18 * fps} durationInFrames={4 * fps}>
        <Outro />
      </Sequence>
    </AbsoluteFill>
  );
};
