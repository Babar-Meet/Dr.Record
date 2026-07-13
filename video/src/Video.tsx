import { AbsoluteFill, Sequence, useVideoConfig } from 'remotion';
import { Intro } from './scenes/Intro';
import { SettingsUI } from './scenes/SettingsUI';
import { Recording } from './scenes/Recording';
import { Outro } from './scenes/Outro';
import { colors } from './theme';

export const MainVideo: React.FC = () => {
  const { fps } = useVideoConfig();

  return (
    <AbsoluteFill style={{ backgroundColor: colors.bg }}>
      <Sequence from={0 * fps} durationInFrames={3 * fps}>
        <Intro />
      </Sequence>
      <Sequence from={3 * fps} durationInFrames={8 * fps}>
        <SettingsUI />
      </Sequence>
      <Sequence from={11 * fps} durationInFrames={6 * fps}>
        <Recording />
      </Sequence>
      <Sequence from={17 * fps} durationInFrames={7 * fps}>
        <Outro />
      </Sequence>
    </AbsoluteFill>
  );
};
