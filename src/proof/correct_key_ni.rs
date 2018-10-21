use std::fmt;
use std::iter;
use std::ops::Shl;
use std::error::Error;
use std::borrow::Borrow;

use ring::digest::{Context, SHA256};
use rayon::prelude::*;

use ::arithimpl::traits::*;
use ::{Paillier, BigInt, EncryptionKey, DecryptionKey, Keypair};
use core::extract_nroot;
use proof::correct_key::compute_digest;
use proof::CorrectKeyProofError;
use std::str::Chars;
// This protocol is based on the NIZK protocol in https://eprint.iacr.org/2018/057.pdf
// for parameters = e = N, m2 = 11, alpha = 6379 see https://eprint.iacr.org/2018/987.pdf
// for full details.

// product of all primes < alpha: https://www.dcode.fr/primorial
const P: &str = "1824183726245393467247644231302244136093537199057104213213550575243641782740360650490963459819244001947254814805714931325851267161710067435807383848463920901137710041673887113990643820798975386714299839914137592590654009952623014982962684955535111234311335565220917383311689115138496765310625882473439402233109450021984891450304679833752756159872219991089194187575068382762952239830901394850887215392132883640669486674102277756753260855640317510235617660944873631054630816035269100510337643250389997837634640249480037184290462924540150133678312185777228630834940021427688892876384196895677819059963882587092166301131529174343474451480089653483180602591751073139733370712300241581635049350925412683097729232092096276490229965785020041921736307394438075266234968515443716828633392848203945374591926800464450599823553052462708727219173990177119684565306222502415160037753326638045687574106534702341439991863742806351468290587722561435038912863815688133288619512790095919904026573249557024839383595481704184528960978957724597263323512030743875614290609368530643094080051166226135271385866188054556684837921935888945641944961066293159525602885452222458958772845494346799890196718717317906330936509091221354615991671869862034179206244894205681566781062633415772628848878715803040358836098609654889521393046492471227546079924219055408612815173193108753184477562256266860297096223934088509777393752624380757072082427603556077039945700711226680778392737267707541904355129695919972995501581794067880959822149963798096452613619855673307435602850208850402301583025111762622381953251883429317603005626232012725708694401272295509035367654620412640848204179955980722707996291909812529974361949926881288349518750747615837667549305083291804187179123453121466640918862622766511668478452223742058912575427337018022812631386313110243745000214354806312441270889672903307645611658893986526812130032112540367173736664288995222516688120866114984318582900331631896931709005163853429427759224323636152573453333607357348169167915027700846002932742550824939007414330697249569916339964247646402851281857942965519194576006169066153524163225631476643914033601957614124583206541834352791003930506139209204661104882701842617501635864883760885236797081996679496751254260706438583316885612406386543479255566185697792942478704336254208839180970748624881039948192415929866204318800220295457932550799088592217150597176505394120909914475575501881459804699385499576326684695531034075283165800622328491384987194944504461864105986907646706095956083156240472489616473946638879726524585936511018780747174387840018674670110430528051586069422163934697899931456041802624175449157279620104126331489491525955411465073551652840009163781923401029513048693746713122813578721687858104388238796796690";
// salt as system parameter
const SALT_STRING : &[u8] = &[75, 90, 101, 110];
const M2: usize = 11;
const DIGEST_SIZE: usize  = 256;
pub struct CorrectKeyProof{
    pub N: BigInt,
    pub sigma_vec: Vec<BigInt>,
}

impl CorrectKeyProof{

    pub fn proof(dk: &DecryptionKey) -> CorrectKeyProof {
        let key_length = &dk.n.bit_length();
        // generate random elements mod N:
       // https://tools.ietf.org/html/rfc8017#appendix-B.2.1
        let msklen = key_length / DIGEST_SIZE;
        let salt_bn = BigInt::from(SALT_STRING);
        let digest_size_bn = BigInt::from(DIGEST_SIZE as u32);

// TODO: use flatten (Morten?)
        let rho_vec = (0..M2).map(|i|{
            let msklen_hash_vec = (0..msklen).map(|j|{
                 compute_digest(
                    iter::once(&dk.n)
                        .chain(iter::once(&BigInt::from(i.clone() as u32)))
                        .chain(iter::once(&BigInt::from(j.clone() as u32)))
                        .chain(iter::once(&salt_bn))
                )
                // concat elements of  msklen_hash_vec to one long element
            }).collect::<Vec<BigInt>>();
            let rho = msklen_hash_vec.iter().zip(0..msklen).fold(BigInt::zero(),|acc, x|{
                acc + x.0.shl(x.1 * DIGEST_SIZE)
            });
            rho % &dk.n
        }).collect::<Vec<BigInt>>();

        let sigma_vec =
            rho_vec.iter()
                .map(|i| {
                    let sigma_i = extract_nroot(dk, i);
                    sigma_i
                }).collect::<Vec<BigInt>>();
        CorrectKeyProof{
            N: dk.n.clone(),
            sigma_vec,
        }

    }

    pub fn verify(&self) -> Result<(), CorrectKeyProofError>{

        let key_length = self.N.bit_length() as usize;
        // generate random elements mod N:
        // https://tools.ietf.org/html/rfc8017#appendix-B.2.1
        let msklen = key_length / DIGEST_SIZE;
        let salt_bn = BigInt::from(SALT_STRING);
        let digest_size_bn = BigInt::from(DIGEST_SIZE as u32);

        // TODO: refactor to a function that accepts size and seed and returns digest of this size
        let rho_vec = (0..M2).map(|i|{
            let msklen_hash_vec = (0..msklen).map(|j|{
                compute_digest(
                    iter::once(&self.N)
                        .chain(iter::once(&BigInt::from(i.clone() as u32)))
                        .chain(iter::once(&BigInt::from(j.clone() as u32)))
                        .chain(iter::once(&salt_bn))
                )
                // concat elements of  msklen_hash_vec to one long element
            }).collect::<Vec<BigInt>>();
            let rho = msklen_hash_vec.iter().zip(0..msklen).fold(BigInt::zero(),|acc, x|{
                acc + x.0.shl(x.1 * DIGEST_SIZE) });
            rho % &self.N
        }).collect::<Vec<BigInt>>();
        let alpha_primorial: BigInt = str::parse(&P).unwrap();
        let gcd_test = alpha_primorial.gcd(&self.N);

        let derived_rho_vec = (0..M2).map(|i|{
            BigInt::modpow(&self.sigma_vec[i], &self.N, &self.N)
        }).collect::<Vec<BigInt>>();

        if rho_vec == derived_rho_vec && gcd_test == BigInt::one(){
            Ok(())
        }
        else { Err(CorrectKeyProofError) }

    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use ::Keypair;
    use Paillier;
    use traits::KeyGeneration;
    //use paillier::*;


    fn test_keypair() -> Keypair {
        let p = str::parse("148677972634832330983979593310074301486537017973460461278300587514468301043894574906886127642530475786889672304776052879927627556769456140664043088700743909632312483413393134504352834240399191134336344285483935856491230340093391784574980688823380828143810804684752914935441384845195613674104960646037368551517").unwrap();
        let q = str::parse("158741574437007245654463598139927898730476924736461654463975966787719309357536545869203069369466212089132653564188443272208127277664424448947476335413293018778018615899291704693105620242763173357203898195318179150836424196645745308205164116144020613415407736216097185962171301808761138424668335445923774195463").unwrap();
        Keypair {
            p: p,
            q: q,
        }
    }

    #[test]
    fn test_correct_zk_proof() {
        //let (ek, dk) = test_keypair().keys();
        let (ek, dk) = Paillier::keypair().keys();
        let test = CorrectKeyProof::proof(&dk);
        assert!(test.verify().is_ok());

    }
}