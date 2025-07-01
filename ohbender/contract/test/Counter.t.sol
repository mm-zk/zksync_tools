// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {Counter} from "../src/Counter.sol";

import {L1VerifierPlonk} from "../src/L1VerifierPlonk.sol";
import {L1VerifierFflonk} from "../src/L1VerifierFflonk.sol";
import {DualVerifier} from "../src/DualVerifier.sol";

contract CounterTest is Test {
    Counter public counter;
    L1VerifierPlonk public l1VerifierPlonk;
    L1VerifierFflonk public l1VerifierFflonk;
    DualVerifier public dualVerifier;

    function setUp() public {
        counter = new Counter();
        counter.setNumber(0);
        l1VerifierPlonk = new L1VerifierPlonk();
        l1VerifierFflonk = new L1VerifierFflonk();
        dualVerifier = new DualVerifier(l1VerifierFflonk, l1VerifierPlonk);
        console.log("Dual verifier address:", address(dualVerifier));
    }

    function test_Increment() public {
        counter.increment();
        assertEq(counter.number(), 1);
    }

    function testFuzz_SetNumber(uint256 x) public {
        counter.setNumber(x);
        assertEq(counter.number(), x);
    }

    function testVerify() public {
        uint256[] memory publicInputs = new uint256[](1);
        publicInputs[
            0
        ] = 7015535880081952002718053721803449349112419973342408527850776172835; // Example public input

        // Example proof for PLONK verification
        uint256[] memory plonkProof = new uint256[](44);
        uint256 i = 0;
        plonkProof[
            i
        ] = 9794115829335635261892009970900983262662328722350043897296947844805068104934;
        i = i + 1;
        plonkProof[
            i
        ] = 6376688084683881026619374537136338297451865846198256586047055586103606209491;
        i = i + 1;
        plonkProof[
            i
        ] = 1919260472292385366210894370861993793879299598521892652958543489850721730544;
        i = i + 1;
        plonkProof[
            i
        ] = 20936457831929032332936264841073513293961515529992516366599854851282384003301;
        i = i + 1;
        plonkProof[
            i
        ] = 21766534381805129644497406511655938509609404690788335887356851449478246694670;
        i = i + 1;
        plonkProof[
            i
        ] = 3140604606520753979466305479032586220158297994591990548822836642889198741036;
        i = i + 1;
        plonkProof[
            i
        ] = 13451161434268558236934944803594456098346874974344957598772476532546159059144;
        i = i + 1;
        plonkProof[
            i
        ] = 8972418033753160263524810565943224008265062692249473789057605150142403848640;
        i = i + 1;
        plonkProof[
            i
        ] = 14566365165952410249450892909622414870975919194081590858881475087747719108354;
        i = i + 1;
        plonkProof[
            i
        ] = 15139657941817394298889092145684109444640877626629242348873073370332958719051;
        i = i + 1;
        plonkProof[
            i
        ] = 3503485793807225866860194193843295657697208089822903550150111550978140282231;
        i = i + 1;
        plonkProof[
            i
        ] = 19798360562106526886193998807081289254853896349455851868607992992060196295754;
        i = i + 1;
        plonkProof[
            i
        ] = 6316628459482576458391511725891582559510472726226029015399668746919446558123;
        i = i + 1;
        plonkProof[
            i
        ] = 2476203931974387449417360285201723124303143659476005277123838366662956471369;
        i = i + 1;
        plonkProof[
            i
        ] = 17696881318877115313058412112726213331596080818091928597758475025574528956266;
        i = i + 1;
        plonkProof[
            i
        ] = 15680998013901152729836071931702579875934617761611690932193666697068510621852;
        i = i + 1;
        plonkProof[
            i
        ] = 5619195503934856627325119733833472075878200918447167624670312757689583959732;
        i = i + 1;
        plonkProof[
            i
        ] = 9434540167364341298681384804805367432971764441908864065468593130870387751380;
        i = i + 1;
        plonkProof[
            i
        ] = 13813039118357577396870732789224015855973172809795918365795972707853803545493;
        i = i + 1;
        plonkProof[
            i
        ] = 18469457761085349814228032812814033873162617963885624086521285470081615225752;
        i = i + 1;
        plonkProof[
            i
        ] = 8807817836888936776178916443403569105682317727101939964078983652120265368671;
        i = i + 1;
        plonkProof[
            i
        ] = 19093693535882972901371717373590832754951298785351849561550952664788165703837;
        i = i + 1;
        plonkProof[
            i
        ] = 12107030308360948847203396113450221080410871613643682812267470379254893781980;
        i = i + 1;
        plonkProof[
            i
        ] = 20220665718278329382988530533467788208064885049648885474257836767007198913589;
        i = i + 1;
        plonkProof[
            i
        ] = 11622744719567381319952242885077399173756385309190308933842604104979918332588;
        i = i + 1;
        plonkProof[
            i
        ] = 6692743227137389047168541891639957961533418346314114588343536689589405694713;
        i = i + 1;
        plonkProof[
            i
        ] = 12004737533816122296011925042077043614377350823914033024307454372378714360340;
        i = i + 1;
        plonkProof[
            i
        ] = 21762283711094920595157866158114085658529948888242914737428997487798240712230;
        i = i + 1;
        plonkProof[
            i
        ] = 9241248013348412011685527845274989644159226257864841794150122474296677351262;
        i = i + 1;
        plonkProof[
            i
        ] = 1282034602892630224602905312714061751316080249588312897370293826523524037292;
        i = i + 1;
        plonkProof[
            i
        ] = 558141051419937849270435457673757180770788599641951660079435195172723565446;
        i = i + 1;
        plonkProof[
            i
        ] = 16561707845348463427568984954092763225352492633725737267880854011616467377152;
        i = i + 1;
        plonkProof[
            i
        ] = 14427863146114303261845669967963031246133245797162461908987966184877475757892;
        i = i + 1;
        plonkProof[
            i
        ] = 3806038851809046127733785504869184727543871026028719772521943032957766132689;
        i = i + 1;
        plonkProof[
            i
        ] = 7816664037770282498006807017264497150414147983774255285136890440259732252058;
        i = i + 1;
        plonkProof[
            i
        ] = 14662329196279850585130424878037403909916573766001057471544468022819862961117;
        i = i + 1;
        plonkProof[
            i
        ] = 5359821837208189822404035971712097574304673634226616534859869486799962145613;
        i = i + 1;
        plonkProof[
            i
        ] = 4931141462457060009125388033160792378307861723638928915994433562503961302312;
        i = i + 1;
        plonkProof[
            i
        ] = 4719774342424176964935892077236606926712750623407240302436270529405063952442;
        i = i + 1;
        plonkProof[
            i
        ] = 17586519719068781935737720935197012383501644271729692557129111984371102557106;
        i = i + 1;
        plonkProof[
            i
        ] = 15357268009437741237798712458503917529861652600560386103468885810325780168200;
        i = i + 1;
        plonkProof[
            i
        ] = 1872780135251596194834296795103577780141332220906704320115362844222036991962;
        i = i + 1;
        plonkProof[
            i
        ] = 21716564152891243892348000496077938079630474503622576739634932203050925574526;
        i = i + 1;
        plonkProof[
            i
        ] = 21791023387565452699738413483231524862195240122365046443519951437785872860469;
        i = i + 1;

        assertTrue(l1VerifierPlonk.verify(publicInputs, plonkProof));

        uint256[] memory dualplonkProof = new uint256[](45);
        dualplonkProof[0] = 1;
        for (uint256 i = 1; i < dualplonkProof.length; i++) {
            dualplonkProof[i] = plonkProof[i - 1];
        }

        assertTrue(dualVerifier.verify(publicInputs, dualplonkProof));

        // and now ohbender verify.
        // original 2 public inputs (full) are:

        uint256[] memory ohbenderPublicInputs = new uint256[](2);
        ohbenderPublicInputs[
            0
        ] = 0x309f3397494dd66536462742c2661015cac60f3efffcbf11c28cdee0691cc6e9;
        ohbenderPublicInputs[
            1
        ] = 0x1e23295c2580561ef4e0589505c3def84e5229ebff17f3a20110615bd65f07c7;

        uint256[] memory ohBenderplonkProof = new uint256[](46);
        ohBenderplonkProof[0] = 2;
        ohBenderplonkProof[1] = 0;
        for (uint256 i = 2; i < ohBenderplonkProof.length; i++) {
            ohBenderplonkProof[i] = plonkProof[i - 2];
        }

        assertTrue(
            dualVerifier.verify(ohbenderPublicInputs, ohBenderplonkProof)
        );
    }

    function test_verifyLongOhBender() public {
        uint256[] memory publicInputs = new uint256[](1);
        publicInputs[
            0
        ] = 5418883971566772847310066135788474133176062119382767039429867968186; // Example public input

        // Example proof for PLONK verification
        uint256[] memory plonkProof = new uint256[](44);
        uint256 i = 0;

        plonkProof[
            i
        ] = 8801314660272923565908421597238036102343998150266931302608856184412643417655;
        i = i + 1;
        plonkProof[
            i
        ] = 10495099717466605641402099835635797434931914611520329263733885347727399943178;
        i = i + 1;
        plonkProof[
            i
        ] = 21876396520666771434907421175015787419397322417867556780640558384837308940354;
        i = i + 1;
        plonkProof[
            i
        ] = 8395786371857916736746404344794395877425553352942848627263677076056608070808;
        i = i + 1;
        plonkProof[
            i
        ] = 19832556866695832288525361435414914257785026678480697131129416302409820960005;
        i = i + 1;
        plonkProof[
            i
        ] = 8046469308915746628738550136737368828559105525905326590213622508924609449455;
        i = i + 1;
        plonkProof[
            i
        ] = 1580364200313683320084644351477965251147444611345585690857243296655201012228;
        i = i + 1;
        plonkProof[
            i
        ] = 5689361254967445088847878960963234977753308019284231557266385366189113515715;
        i = i + 1;
        plonkProof[
            i
        ] = 6317672262974286338458148462194691608046567169305059722597611887614380820416;
        i = i + 1;
        plonkProof[
            i
        ] = 5831838997942642282527575720662315695333339336454143387061769149027459706677;
        i = i + 1;
        plonkProof[
            i
        ] = 13122899529073815200794621557477372442672849725571998337688510724753263065742;
        i = i + 1;
        plonkProof[
            i
        ] = 8986693795841825478742911969091681669386108450093514524287974541283855061783;
        i = i + 1;
        plonkProof[
            i
        ] = 4466989138313434268618261484457151403512592752111538306625735046020077013590;
        i = i + 1;
        plonkProof[
            i
        ] = 10670972719901758801600600382495734343486367654238332859889615309842425615540;
        i = i + 1;
        plonkProof[
            i
        ] = 10222150765507780492365485629700766134420299104024488167418646075323735626596;
        i = i + 1;
        plonkProof[
            i
        ] = 18662798830048132772243213338992072563598720426747629073849507897878454282031;
        i = i + 1;
        plonkProof[
            i
        ] = 2457261071288849260908899706932257070361824057508248718939180009388711776975;
        i = i + 1;
        plonkProof[
            i
        ] = 10611005199283459696166812734292146527198071948212387168461142703244128560444;
        i = i + 1;
        plonkProof[
            i
        ] = 789691287793091562173910150711584920312465732316606941076590581923736755586;
        i = i + 1;
        plonkProof[
            i
        ] = 3977121531615559427955279054574696838078810010850350047576034327359526361351;
        i = i + 1;
        plonkProof[
            i
        ] = 9686725904444274359307465100194930096125759761644285839265894882897573569674;
        i = i + 1;
        plonkProof[
            i
        ] = 3108749239238084724145971232228066256859983752311068746072746335833181963289;
        i = i + 1;
        plonkProof[
            i
        ] = 21162287712779273424529779317270522582035866126238886417525804723146292042981;
        i = i + 1;
        plonkProof[
            i
        ] = 19681987947501966641001074080187066690148204224813673576687976242176844260617;
        i = i + 1;
        plonkProof[
            i
        ] = 6289059135237408194887601782695981651650675839461412716601281190833655265240;
        i = i + 1;
        plonkProof[
            i
        ] = 17887557304709465002385186442713272607886306294631838062742296341673090391344;
        i = i + 1;
        plonkProof[
            i
        ] = 14658128405001517619048211314081011808888885266795590335400751849787078767720;
        i = i + 1;
        plonkProof[
            i
        ] = 9113858119463671249152594097581198814652444657820490800555410921105051540044;
        i = i + 1;
        plonkProof[
            i
        ] = 15821653214528049405054817327937161877998169523965464992816328685129232869966;
        i = i + 1;
        plonkProof[
            i
        ] = 176540689387041276405254225919805187727920553708831767042457963254170569767;
        i = i + 1;
        plonkProof[
            i
        ] = 21015847453128856323409968238795168720422990136247104160344326742546551003789;
        i = i + 1;
        plonkProof[
            i
        ] = 18353645542819874004331645105084513833805673579337639242204698495653044117019;
        i = i + 1;
        plonkProof[
            i
        ] = 5051605594444271997046135900847369642096912813389345371997768718967160177125;
        i = i + 1;
        plonkProof[
            i
        ] = 1308273734932839149795152844660246995306741510462001716107699955779658130167;
        i = i + 1;
        plonkProof[
            i
        ] = 3439561656024952124983554486043619095655665474317396607016340054293211201669;
        i = i + 1;
        plonkProof[
            i
        ] = 10361695726640992611924718833053685674848014393948332844004666961949267755391;
        i = i + 1;
        plonkProof[
            i
        ] = 17881933793478233925366847147555146996259961158246582611679094251085460722277;
        i = i + 1;
        plonkProof[
            i
        ] = 13103470957277611095814894797993082366974943801430979346659894871688902211730;
        i = i + 1;
        plonkProof[
            i
        ] = 18050983302052013903565147083165724365350045276218102033333677163572442841841;
        i = i + 1;
        plonkProof[
            i
        ] = 459100387577307706400751474309125393605409851780227979305971124199582437845;
        i = i + 1;
        plonkProof[
            i
        ] = 16951163945640300017863675807940168487319234827126988642196195385009187308026;
        i = i + 1;
        plonkProof[
            i
        ] = 1475254597832865730163500984856186570346158636149911797254392119902362631737;
        i = i + 1;
        plonkProof[
            i
        ] = 5699323526300370234372769163883785523688004083837663007807991480611143866764;
        i = i + 1;
        plonkProof[
            i
        ] = 12010778538135801215229987035193581271877459262543408887094238647043389124456;
        i = i + 1;
        assertTrue(l1VerifierPlonk.verify(publicInputs, plonkProof));

        // and now ohbender verify.
        // original 2 public inputs (full) are:

        uint256[] memory ohbenderPublicInputs = new uint256[](2);
        ohbenderPublicInputs[
            0
        ] = 0x1e23295c2580561ef4e0589505c3def84e5229ebff17f3a20110615bd65f07c7;
        ohbenderPublicInputs[
            1
        ] = 0xccf81fbae45e28a044e1864c403630d4c52dd090153d6953d44658064ec42e61;

        uint256[] memory ohBenderplonkProof = new uint256[](46);
        ohBenderplonkProof[0] = 2;
        ohBenderplonkProof[
            1
        ] = 0x309f3397494dd66536462742c2661015cac60f3efffcbf11c28cdee0691cc6e9;
        for (uint256 i = 2; i < ohBenderplonkProof.length; i++) {
            ohBenderplonkProof[i] = plonkProof[i - 2];
        }

        assertTrue(
            dualVerifier.verify(ohbenderPublicInputs, ohBenderplonkProof)
        );
    }

    function reverseUint256(
        uint256 input
    ) internal pure returns (uint256 result) {
        bytes memory out = new bytes(32);

        for (uint256 i = 0; i < 8; i++) {
            uint32 chunk = uint32(input >> (32 * (7 - i))); // get 4-byte chunk
            // Reverse 4 bytes of the chunk
            out[i * 4 + 0] = bytes1(uint8(chunk >> 0));
            out[i * 4 + 1] = bytes1(uint8(chunk >> 8));
            out[i * 4 + 2] = bytes1(uint8(chunk >> 16));
            out[i * 4 + 3] = bytes1(uint8(chunk >> 24));
        }

        assembly {
            result := mload(add(out, 32))
        }
    }

    function keccakTwoUint256(
        uint256 a,
        uint256 b
    ) public pure returns (uint256) {
        uint256 a_be = reverseUint256(a);
        console.logBytes32(bytes32(a_be));
        uint256 b_be = reverseUint256(b);
        bytes32 hash = keccak256(abi.encodePacked(a_be, b_be));
        return reverseUint256(uint256(hash));
    }

    function test_reversedKeccak() public {
        uint256[] memory ohbenderPublicInputs = new uint256[](2);
        ohbenderPublicInputs[
            0
        ] = 0x309f3397494dd66536462742c2661015cac60f3efffcbf11c28cdee0691cc6e9;
        ohbenderPublicInputs[
            1
        ] = 0x1e23295c2580561ef4e0589505c3def84e5229ebff17f3a20110615bd65f07c7;

        uint256 result = keccakTwoUint256(
            ohbenderPublicInputs[0],
            ohbenderPublicInputs[1]
        );
        console.logBytes32(bytes32(result));
    }
}
