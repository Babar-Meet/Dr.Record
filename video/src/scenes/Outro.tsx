import { AbsoluteFill, spring, useCurrentFrame, useVideoConfig, Img, staticFile } from 'remotion';
import { colors, fonts } from '../theme';

export const Outro: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const logoSpring = spring({ frame, fps, config: { damping: 12, mass: 0.5 } });
  const titleSpring = spring({ frame: frame - 10, fps, config: { damping: 12, mass: 0.5 } });
  const taglineSpring = spring({ frame: frame - 20, fps, config: { damping: 12, mass: 0.5 } });
  const descSpring = spring({ frame: frame - 35, fps, config: { damping: 12, mass: 0.5 } });

  return (
    <AbsoluteFill style={{ justifyContent: 'center', alignItems: 'center', padding: 60 }}>
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 20 }}>
        <Img
          src={staticFile('/icon.png')}
          style={{
            width: 72,
            height: 72,
            borderRadius: 16,
            opacity: logoSpring,
            transform: `scale(${logoSpring})`,
            boxShadow: `0 0 30px ${colors.accent}22`,
          }}
        />

        <h2
          style={{
            fontFamily: fonts.ui,
            fontSize: 36,
            fontWeight: 700,
            color: colors.textBright,
            textAlign: 'center',
            margin: 0,
            opacity: titleSpring,
            transform: `translateY(${(1 - titleSpring) * 30}px)`,
          }}
        >
          Dr. Record
        </h2>

        <p
          style={{
            fontFamily: fonts.ui,
            fontSize: 20,
            color: colors.accent,
            textAlign: 'center',
            margin: 0,
            fontWeight: 500,
            opacity: taglineSpring,
            transform: `translateY(${(1 - taglineSpring) * 20}px)`,
          }}
        >
          Set it up. Forget it. It just works.
        </p>

        <div style={{ width: 60, height: 2, background: colors.border, margin: '4px 0', opacity: taglineSpring }} />

        <p
          style={{
            fontFamily: fonts.ui,
            fontSize: 16,
            color: colors.textDim,
            textAlign: 'center',
            maxWidth: 520,
            lineHeight: 1.7,
            margin: '4px 0 0',
            opacity: descSpring,
          }}
        >
          One-time setup. Always there when you need it.
          <br />
          <span style={{ color: colors.accent }}>Built for Windows. Designed for you.</span>
        </p>

        <p
          style={{
            fontFamily: fonts.ui,
            fontSize: 13,
            color: colors.textDim,
            textAlign: 'center',
            marginTop: 20,
            opacity: spring({ frame: frame - 60, fps, config: { damping: 12, mass: 0.5 } }),
          }}
        >
          Because how you record matters.
        </p>
      </div>
    </AbsoluteFill>
  );
};
